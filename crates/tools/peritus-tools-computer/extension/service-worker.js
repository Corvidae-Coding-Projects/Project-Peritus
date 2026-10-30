const NATIVE_HOST = 'com.corvidae.peritus';
const SESSION_KEY = 'peritusSessions';
const PAUSE_KEY = 'peritusPaused';
const GRANTS_KEY = 'peritusHttpsTabs';
const LEASE_MS = 10 * 60 * 1000;
const MAX_SESSIONS = 32;
const MAX_TEXT = 20000;
const MAX_ELEMENTS = 200;

let nativePort = null;
let paused = false;
let sessions = {};
let httpsTabs = {};
let bridgeError = null;

function loopbackUrl(value) {
  let parsed;
  try {
    parsed = new URL(value);
  } catch (_) {
    return false;
  }
  const host = parsed.hostname.toLowerCase();
  return (parsed.protocol === 'http:' || parsed.protocol === 'https:') &&
    !parsed.username && !parsed.password &&
    (host === 'localhost' || host === '127.0.0.1' || host === '[::1]' || host === '::1');
}

function safeUrl(value) {
  try {
    const url = new URL(value);
    return !url.username && !url.password && (url.protocol === 'https:' || loopbackUrl(value));
  } catch (_) { return false; }
}

function permittedTab(tab) {
  if (loopbackUrl(tab.url || '')) return true;
  return safeUrl(tab.url || '') && httpsTabs[String(tab.id)] === new URL(tab.url).origin;
}

function cleanError(error) {
  return String(error && error.message ? error.message : error).slice(0, 3000);
}

async function persistSessions() {
  await chrome.storage.session.set({[SESSION_KEY]: sessions});
}

async function restoreState() {
  const local = await chrome.storage.local.get(PAUSE_KEY);
  paused = local[PAUSE_KEY] === true;
  const retained = await chrome.storage.session.get([SESSION_KEY, GRANTS_KEY]);
  httpsTabs = retained[GRANTS_KEY] || {};
  sessions = retained[SESSION_KEY] || {};
}

function connectNative() {
  if (paused || nativePort) return;
  try {
    nativePort = chrome.runtime.connectNative(NATIVE_HOST);
    bridgeError = null;
    nativePort.onMessage.addListener(message => {
      void dispatch(message).then(result => {
        if (nativePort) nativePort.postMessage({...result, _bridge_id: message._bridge_id});
      }).catch(error => {
        if (nativePort) nativePort.postMessage({success: false, error: cleanError(error), _bridge_id: message._bridge_id});
      });
    });
    nativePort.onDisconnect.addListener(() => {
      bridgeError = chrome.runtime.lastError ? chrome.runtime.lastError.message : 'Native host disconnected';
      void chrome.storage.local.set({peritusBridgeError: bridgeError});
      nativePort = null;
      if (!paused) chrome.alarms.create('peritus-reconnect', {delayInMinutes: 0.05});
    });
  } catch (error) {
    bridgeError = cleanError(error);
    void chrome.storage.local.set({peritusBridgeError: bridgeError});
    nativePort = null;
    if (!paused) chrome.alarms.create('peritus-reconnect', {delayInMinutes: 0.1});
  }
}

async function eligibleTabs() {
  const tabs = await chrome.tabs.query({});
  return tabs.filter(tab => tab.id !== undefined && permittedTab(tab)).sort((a, b) => a.id - b.id);
}

async function requireSession(identifier) {
  const session = sessions[identifier];
  if (!session) throw new Error('Browser session is unavailable; extension_open or extension_attach again');
  const tab = await chrome.tabs.get(session.activeTab).catch(() => null);
  if (!tab) {
    delete sessions[identifier];
    await persistSessions();
    throw new Error('Selected Chrome tab no longer exists; open or attach again');
  }
  if (!permittedTab(tab)) {
    delete sessions[identifier];
    await persistSessions();
    throw new Error('Selected tab left its authorized origin or access was revoked; select it again in the extension popup');
  }
  session.expires = Date.now() + LEASE_MS;
  return {session, tab};
}

async function waitForTab(tabId) {
  const start = Date.now();
  while (Date.now() - start < 20000) {
    const tab = await chrome.tabs.get(tabId);
    if (tab.status === 'complete') return tab;
    await new Promise(resolve => setTimeout(resolve, 75));
  }
  throw new Error('Chrome navigation did not complete within 20 seconds');
}

async function waitForNavigation(tabId, previousUrl) {
  const start = Date.now();
  while (Date.now() - start < 20000) {
    const tab = await chrome.tabs.get(tabId);
    if (tab.url !== previousUrl && tab.status === 'complete') return tab;
    await new Promise(resolve => setTimeout(resolve, 75));
  }
  throw new Error('Chrome navigation did not reach a new completed URL within 20 seconds');
}

async function pageObservation(tabId, selector, token) {
  const results = await chrome.scripting.executeScript({
    target: {tabId, frameIds: [0]},
    world: 'ISOLATED',
    args: [selector || null, token, MAX_ELEMENTS, MAX_TEXT],
    func: (requestedSelector, observation, elementLimit, textLimit) => {
      const fallback = 'input,textarea,select,button,a[href],[role="button"],[role="checkbox"],[contenteditable="true"]';
      let nodes;
      try {
        nodes = Array.from(document.querySelectorAll(requestedSelector || fallback));
      } catch (error) {
        throw new Error(`Invalid observation selector: ${error.message}`);
      }
      document.querySelectorAll('[data-peritus-local-ref]').forEach(node => node.removeAttribute('data-peritus-local-ref'));
      const elements = [];
      let omitted = nodes.length > elementLimit;
      for (const node of nodes) {
        if (elements.length >= elementLimit) break;
        const rect = node.getBoundingClientRect();
        const type = typeof node.type === 'string' ? node.type : null;
        if (!(rect.width > 0 && rect.height > 0) && type !== 'file') continue;
        const ref = `${observation}-${elements.length + 1}`;
        node.setAttribute('data-peritus-local-ref', ref);
        const labels = node.labels ? Array.from(node.labels).map(label => label.innerText).join(' ') : '';
        const name = node.getAttribute('aria-label') || labels || node.innerText || node.getAttribute('placeholder') || node.getAttribute('name') || node.id || '';
        elements.push({
          ref,
          frame: 0,
          tag: node.tagName.toLowerCase(),
          role: node.getAttribute('role'),
          name: String(name).slice(0, 256),
          type,
          value: type === 'password' ? '[password]' : ('value' in node ? String(node.value || '').slice(0, 512) : null),
          checked: typeof node.checked === 'boolean' ? node.checked : null,
          disabled: Boolean(node.disabled),
          box: {x: rect.x, y: rect.y, width: rect.width, height: rect.height}
        });
      }
      return {
        text: document.body ? document.body.innerText.slice(0, textLimit) : '',
        elements,
        elements_omitted: omitted,
        viewport: {width: window.innerWidth, height: window.innerHeight, deviceScaleFactor: window.devicePixelRatio || 1}
      };
    }
  });
  if (!results.length || !results[0].result) throw new Error('Chrome did not return a top-frame observation');
  return results[0].result;
}

async function capture(tab) {
  const target = {tabId: tab.id};
  await chrome.debugger.attach(target, '1.3');
  try {
    const capture = await chrome.debugger.sendCommand(target, 'Page.captureScreenshot', {
      format: 'jpeg',
      quality: 62,
      fromSurface: true,
      captureBeyondViewport: false
    });
    return `data:image/jpeg;base64,${capture.data}`;
  } finally {
    await chrome.debugger.detach(target).catch(() => {});
  }
}

async function observe(identifier, selector) {
  const {session, tab} = await requireSession(identifier);
  const observation = crypto.randomUUID().replaceAll('-', '');
  const page = await pageObservation(tab.id, selector, observation);
  const screenshot = await capture(tab);
  session.observation = observation;
  await persistSessions();
  const tabs = [];
  for (let index = 0; index < session.tabs.length; index += 1) {
    const candidate = await chrome.tabs.get(session.tabs[index].id).catch(() => null);
    if (candidate && permittedTab(candidate)) tabs.push({index, url: candidate.url || '', owned: session.tabs[index].owned});
  }
  const current = await chrome.tabs.get(tab.id);
  if (!sessions[identifier] || !permittedTab(current) || current.url !== tab.url) {
    throw new Error('Selected tab changed or access was revoked during observation; select it again');
  }
  return {
    success: true,
    backend: 'chrome_extension',
    kind: 'browser',
    session: identifier,
    observation,
    owned: session.tabs.every(candidate => candidate.owned),
    url: tab.url,
    title: tab.title || '',
    text: page.text,
    elements: page.elements,
    elements_omitted: page.elements_omitted,
    viewport: page.viewport,
    screenshot,
    events: [],
    downloads: [],
    dialog: null,
    tabs
  };
}

async function pageAction(tabId, action, request) {
  const result = await chrome.scripting.executeScript({
    target: {tabId, frameIds: [0]},
    world: 'ISOLATED',
    args: [action, request],
    func: (kind, args) => {
      const escaped = value => globalThis.CSS && CSS.escape ? CSS.escape(value) : value.replace(/[^a-zA-Z0-9_-]/g, '\\$&');
      const node = args.ref ? document.querySelector(`[data-peritus-local-ref="${escaped(args.ref)}"]`) : null;
      if (args.ref && !node) throw new Error('Element reference is unavailable or detached; observe again');
      const notify = (target, name) => target.dispatchEvent(new Event(name, {bubbles: true, composed: true}));
      if (kind === 'click') {
        const target = node || document.elementFromPoint(args.x, args.y);
        if (!target) throw new Error('No element exists at the requested coordinates');
        target.click();
      } else if (kind === 'fill') {
        if (!node || !(node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement || node.isContentEditable)) throw new Error('fill requires an observed editable element ref');
        if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) {
          const prototype = node instanceof HTMLInputElement ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
          Object.getOwnPropertyDescriptor(prototype, 'value').set.call(node, args.text);
        } else node.textContent = args.text;
        notify(node, 'input');
        notify(node, 'change');
      } else if (kind === 'select') {
        if (!(node instanceof HTMLSelectElement)) throw new Error('select requires an observed select ref');
        node.value = args.text;
        notify(node, 'input');
        notify(node, 'change');
      } else if (kind === 'check') {
        if (!(node instanceof HTMLInputElement) || (node.type !== 'checkbox' && node.type !== 'radio')) throw new Error('check requires an observed checkbox or radio ref');
        node.checked = args.checked !== false;
        notify(node, 'input');
        notify(node, 'change');
      } else if (kind === 'press') {
        const target = node || document.activeElement || document.body;
        const key = args.key;
        const down = new KeyboardEvent('keydown', {key, code: key, bubbles: true, cancelable: true});
        const allowed = target.dispatchEvent(down);
        target.dispatchEvent(new KeyboardEvent('keyup', {key, code: key, bubbles: true}));
        if (allowed && key === 'Enter' && target.form) target.form.requestSubmit();
      } else if (kind === 'scroll') {
        window.scrollBy({left: args.x || 0, top: args.y === undefined ? 600 : args.y, behavior: 'instant'});
      } else if (kind === 'navigate_location') {
        setTimeout(() => window.location.assign(args.url), 0);
      } else if (kind === 'history_back') {
        setTimeout(() => window.history.back(), 0);
      } else if (kind === 'history_forward') {
        setTimeout(() => window.history.forward(), 0);
      } else throw new Error('Unsupported extension page action');
      return true;
    }
  });
  if (!result.length) throw new Error('Chrome did not execute the page action');
}

async function closeSession(identifier) {
  const session = sessions[identifier];
  if (!session) return {success: true, closed: identifier, already_closed: true};
  for (const tab of session.tabs) {
    if (tab.owned) await chrome.tabs.remove(tab.id).catch(() => {});
  }
  const preserved = session.tabs.some(tab => !tab.owned);
  delete sessions[identifier];
  await persistSessions();
  return {success: true, closed: identifier, existing_browser_preserved: preserved};
}

async function dispatch(request) {
  if (!request || typeof request !== 'object' || Array.isArray(request)) throw new Error('Invalid bridge request');
  const action = request.action;
  if (action === 'status') return {success: true, connected: true, paused, sessions: Object.keys(sessions).length, bridge_error: bridgeError};
  if (paused) throw new Error('Peritus browser bridge is paused from the Chrome extension popup');
  if (action === 'extension_tabs') {
    const candidates = await eligibleTabs();
    return {success: true, backend: 'chrome_extension', tabs: candidates.map((tab, index) => ({index, title: tab.title || '', url: tab.url || ''}))};
  }
  if (action === 'extension_open') {
    if (Object.keys(sessions).length >= MAX_SESSIONS) throw new Error('Extension session limit reached; close an existing session');
    if (!safeUrl(request.url)) throw new Error('Use credential-free HTTPS, or HTTP(S) loopback URLs');
    const tab = await chrome.tabs.create({url: request.url, active: request.visible !== false});
    await waitForTab(tab.id);
    const loaded = await chrome.tabs.get(tab.id);
    if (!permittedTab(loaded)) return {success:false,failure:'browser_tab_selection_required',url:loaded.url,
      next_action:'In the Peritus Chrome popup, select Share this HTTPS tab, then extension_tabs and extension_attach.'};
    const identifier = `extension:${crypto.randomUUID().replaceAll('-', '')}`;
    sessions[identifier] = {activeTab: tab.id, tabs: [{id: tab.id, owned: true}], observation: null, expires: Date.now() + LEASE_MS};
    await persistSessions();
    return observe(identifier, request.selector);
  }
  if (action === 'extension_attach') {
    if (Object.keys(sessions).length >= MAX_SESSIONS) throw new Error('Extension session limit reached; close an existing session');
    const candidates = await eligibleTabs();
    const index = Number.isInteger(request.tab) ? request.tab : 0;
    if (index < 0 || index >= candidates.length) throw new Error(`Selected authorized tab does not exist; ${candidates.length} eligible tab(s) are open`);
    const tab = candidates[index];
    const identifier = `extension:${crypto.randomUUID().replaceAll('-', '')}`;
    sessions[identifier] = {activeTab: tab.id, tabs: [{id: tab.id, owned: false}], observation: null, expires: Date.now() + LEASE_MS};
    await persistSessions();
    return observe(identifier, request.selector);
  }
  if (action === 'read') {
    const {tab} = await requireSession(request.session);
    const requested = new URL(request.url); requested.hash = '';
    const actual = new URL(tab.url); actual.hash = '';
    if (requested.href !== actual.href) throw new Error('Selected tab URL differs from the requested web source');
    const result = await chrome.scripting.executeScript({target:{tabId:tab.id,frameIds:[0]},world:'ISOLATED',
      func:() => ({url:location.href,html:document.documentElement.outerHTML})});
    const document = result[0]?.result;
    if (!document || new URL(document.url).origin !== actual.origin) throw new Error('Selected document changed during the read; select and observe it again');
    if (new TextEncoder().encode(document.html).length > 2 * 1024 * 1024) throw new Error('Selected-tab HTML exceeds the 2 MiB bridge bound; use native/API reading');
    // Recheck authorization after the asynchronous document read.
    const current = await chrome.tabs.get(tab.id);
    if (!permittedTab(current) || current.url !== document.url) throw new Error('Selected tab navigated during the read; observe it again');
    return {success:true,...document,http_status:null,method:'browser_tab'};
  }
  if (action === 'observe') return observe(request.session, request.selector);
  if (action === 'close') return closeSession(request.session);
  const {session, tab} = await requireSession(request.session);
  if (!loopbackUrl(tab.url || '')) throw new Error('Selected external HTTPS tabs support read and observe only');
  if (request.observation !== session.observation) throw new Error('Stale or missing observation; observe this session before acting');
  if (action === 'navigate') {
    if (!loopbackUrl(request.url)) throw new Error('Controlled navigation is restricted to loopback; select external HTTPS tabs in the popup');
    await pageAction(tab.id, 'navigate_location', request);
    await waitForNavigation(tab.id, tab.url);
  } else if (['click', 'fill', 'press', 'select', 'check', 'scroll'].includes(action)) {
    await pageAction(tab.id, action, request);
  } else if (action === 'new_tab') {
    if (!loopbackUrl(request.url)) throw new Error('New controlled tabs require loopback; use extension_open and select external HTTPS tabs in the popup');
    const added = await chrome.tabs.create({url: request.url, active: request.visible !== false, windowId: tab.windowId});
    await waitForTab(added.id);
    session.tabs.push({id: added.id, owned: true});
    session.activeTab = added.id;
  } else if (action === 'switch_tab') {
    const index = Number.isInteger(request.tab) ? request.tab : -1;
    if (index < 0 || index >= session.tabs.length) throw new Error('Selected extension session tab does not exist');
    session.activeTab = session.tabs[index].id;
  } else if (action === 'back') {
    await pageAction(tab.id, 'history_back', request);
    await waitForNavigation(tab.id, tab.url);
  } else if (action === 'forward') {
    await pageAction(tab.id, 'history_forward', request);
    await waitForNavigation(tab.id, tab.url);
  } else if (action === 'reload') {
    await chrome.tabs.reload(tab.id);
    await waitForTab(tab.id);
  } else {
    throw new Error('This action is unavailable in the local Chrome extension backend');
  }
  await persistSessions();
  return observe(request.session, request.selector);
}

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (sender.url !== chrome.runtime.getURL('popup.html')) return false;
  if (message && (message.kind === 'popup-share' || message.kind === 'popup-revoke')) {
    void (async () => {
      const [tab] = await chrome.tabs.query({active:true,currentWindow:true});
      if (!tab || !safeUrl(tab.url || '') || new URL(tab.url).protocol !== 'https:') throw new Error('Select a credential-free HTTPS tab first');
      if (message.kind === 'popup-share') {
        // executeScript proves Chrome granted activeTab for this explicit popup invocation.
        await chrome.scripting.executeScript({target:{tabId:tab.id,frameIds:[0]},func:() => location.origin});
        httpsTabs[String(tab.id)] = new URL(tab.url).origin;
      } else {
        delete httpsTabs[String(tab.id)];
        for (const [id, session] of Object.entries(sessions)) {
          if (session.tabs.some(candidate => candidate.id === tab.id)) { delete sessions[id]; }
        }
        await persistSessions();
      }
      await chrome.storage.session.set({[GRANTS_KEY]:httpsTabs});
      sendResponse({success:true,shared:message.kind === 'popup-share',origin:new URL(tab.url).origin});
    })().catch(error => sendResponse({success:false,error:cleanError(error)}));
    return true;
  }
  if (message && message.kind === 'popup-status') {
    sendResponse({connected: nativePort !== null, paused, sessions: Object.keys(sessions).length, sharedHttpsTabs: Object.keys(httpsTabs).length, bridgeError});
    return false;
  }
  if (message && message.kind === 'popup-pause') {
    void (async () => {
      paused = message.paused === true;
      await chrome.storage.local.set({[PAUSE_KEY]: paused});
      if (paused && nativePort) {
        const port = nativePort;
        nativePort = null;
        port.disconnect();
      } else if (!paused) connectNative();
      sendResponse({success: true, paused});
    })();
    return true;
  }
  return false;
});

chrome.alarms.onAlarm.addListener(alarm => {
  if (alarm.name === 'peritus-reconnect') connectNative();
  if (alarm.name === 'peritus-lease-cleanup') {
    const now = Date.now();
    for (const [identifier, session] of Object.entries(sessions)) {
      if (session.expires < now) void closeSession(identifier);
    }
  }
});

chrome.runtime.onInstalled.addListener(() => { void restoreState().then(connectNative); });
chrome.runtime.onStartup.addListener(() => { void restoreState().then(connectNative); });
chrome.alarms.create('peritus-lease-cleanup', {periodInMinutes: 1});
void restoreState().then(connectNative);

chrome.tabs.onRemoved.addListener(tabId => {
  delete httpsTabs[String(tabId)];
  void chrome.storage.session.set({[GRANTS_KEY]:httpsTabs});
});
chrome.tabs.onUpdated.addListener((tabId, changes) => {
  if (changes.url && httpsTabs[String(tabId)] && (!safeUrl(changes.url) || new URL(changes.url).origin !== httpsTabs[String(tabId)])) {
    delete httpsTabs[String(tabId)];
    void chrome.storage.session.set({[GRANTS_KEY]:httpsTabs});
  }
});
