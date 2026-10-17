"""Persistent real Playwright browser sessions; no fabricated pages or test assertions."""
import hashlib
import inspect
import json
from pathlib import Path
import shutil
from urllib.parse import unquote, urlparse
import uuid


class Browsers:
    def __init__(self, root, workspace, screenshot):
        self.root = root
        self.workspace = workspace.resolve()
        self.screenshot = screenshot
        self.playwright = None
        self.sessions = {}
        self.reader = None

    def engine(self):
        if self.playwright is None:
            from playwright.sync_api import sync_playwright
            self.playwright = sync_playwright().start()
        return self.playwright

    def url(self, value):
        parsed = urlparse(value)
        if parsed.scheme not in ('http', 'https', 'file') and value != 'about:blank':
            raise ValueError('Use an http, https, workspace file URL, or about:blank')
        if parsed.scheme == 'file':
            self.file(unquote(parsed.path))
        return value

    def file(self, value):
        path = Path(value)
        path = (self.workspace / path).resolve() if not path.is_absolute() else path.resolve()
        if not path.is_relative_to(self.workspace) or not path.is_file():
            raise ValueError('Upload/file URLs must name a regular file inside the selected workspace')
        return path

    def read_web(self, args):
        """Read a public page in a temporary context, without user profiles or file access."""
        from playwright.sync_api import TimeoutError as PlaywrightTimeout
        value = args['url']
        parsed = urlparse(value)
        if parsed.scheme not in ('http', 'https') or not parsed.hostname or parsed.username or parsed.password:
            raise ValueError('Web reading supports HTTP(S) without embedded credentials only')
        if self.reader is None or not self.reader.is_connected():
            executable = shutil.which('google-chrome') or shutil.which('chromium') or shutil.which('chromium-browser')
            self.reader = self.engine().chromium.launch(
                headless=True, executable_path=executable, chromium_sandbox=True)
        context = None
        try:
            context = self.reader.new_context(accept_downloads=False, service_workers='block')
            def route(request_route):
                url = urlparse(request_route.request.url)
                if url.scheme in ('http', 'https') and not url.username and not url.password:
                    request_route.continue_()
                else:
                    request_route.abort()
            context.route('**/*', route)
            page = context.new_page()
            context.on('page', lambda extra: extra.close())
            page.on('dialog', lambda dialog: dialog.dismiss())
            response = page.goto(value, wait_until='domcontentloaded', timeout=20000)
            try:
                page.wait_for_load_state('networkidle', timeout=3000)
            except PlaywrightTimeout:
                pass  # Polling pages need not become network-idle to be readable.
            page.wait_for_timeout(min(5000, max(0, args.get('wait_ms', 500))))
            final = urlparse(page.url)
            if final.scheme not in ('http', 'https') or final.username or final.password:
                raise ValueError('Unsafe web navigation was rejected')
            html = page.content().encode('utf-8')
            if len(html) > 64 * 1024 * 1024:
                raise ValueError('Rendered source exceeds the 64 MiB resource bound')
            directory = self.root / 'web-readings'
            directory.mkdir(mode=0o700, exist_ok=True)
            path = directory / (uuid.uuid4().hex + '.html')
            path.write_bytes(html)
            return {'success': True, 'path': str(path), 'bytes': len(html),
                    'sha256': hashlib.sha256(html).hexdigest(), 'url': page.url,
                    'status': response.status if response else 0}
        finally:
            try:
                if context is not None:
                    context.close()
            finally:
                self.reader.close()
                self.reader = None

    def control(self, args):
        action = args['action']
        if action == 'open':
            identifier = uuid.uuid4().hex
            directory = self.root / identifier
            directory.mkdir(mode=0o700)
            executable = shutil.which('google-chrome') or shutil.which('chromium') or shutil.which('chromium-browser')
            context = self.engine().chromium.launch_persistent_context(
                str(directory / 'profile'), executable_path=executable,
                headless=not args.get('visible', False), viewport={'width': 1280, 'height': 900},
                accept_downloads=True, chromium_sandbox=True,
            )
            page = context.pages[0] if context.pages else context.new_page()
            session = {'context': context, 'browser': None, 'page': page, 'owned': True, 'directory': directory, 'refs': {}, 'events': [], 'downloads': []}
            self.sessions[identifier] = session
            self.watch(session, page)
            try:
                page.goto(self.url(args.get('url', 'about:blank')), wait_until='domcontentloaded', timeout=20000)
                return self.observe(identifier)
            except Exception:
                self.close(identifier)
                raise
        if action == 'attach':
            endpoint = args['endpoint']
            parsed = urlparse(endpoint)
            if parsed.scheme not in ('http', 'ws') or parsed.hostname not in ('localhost', '127.0.0.1', '::1'):
                raise ValueError('Attach requires an explicit loopback Chrome DevTools endpoint')
            options = {'timeout': 15000}
            if 'no_defaults' in inspect.signature(self.engine().chromium.connect_over_cdp).parameters:
                options['no_defaults'] = True
            else:
                raise ValueError('This Playwright version cannot attach without changing browser defaults; update with peritus automation setup')
            browser = self.engine().chromium.connect_over_cdp(endpoint, **options)
            pages = [page for context in browser.contexts for page in context.pages]
            index = args.get('tab', 0)
            if index < 0 or index >= len(pages):
                browser.close()
                raise ValueError('Selected browser tab does not exist')
            identifier = uuid.uuid4().hex
            directory = self.root / identifier
            directory.mkdir(mode=0o700)
            page = pages[index]
            self.sessions[identifier] = {'context': page.context, 'browser': browser, 'page': page, 'owned': False, 'directory': directory, 'refs': {}, 'events': [], 'downloads': []}
            self.watch(self.sessions[identifier], page)
            return self.observe(identifier)
        identifier = args['session']
        session = self.sessions.get(identifier)
        if session is None:
            raise ValueError('Browser session is unavailable; open/attach again and observe fresh state')
        if action == 'observe':
            return self.observe(identifier, args.get('selector'))
        if action == 'close':
            self.close(identifier)
            return {'success': True, 'closed': identifier, 'existing_browser_preserved': not session['owned']}
        if args.get('observation') != session.get('observation'):
            raise ValueError('Stale or missing observation; observe this session before acting')
        page = session['page']
        node = None
        if args.get('ref'):
            node = session['refs'].get(args['ref'])
            if node is None or not node.evaluate('(e) => e.isConnected'):
                raise ValueError('Element reference is unavailable or detached; observe again')
        if action == 'navigate':
            page.goto(self.url(args['url']), wait_until='domcontentloaded', timeout=20000)
        elif action == 'click':
            if node:
                node.click(timeout=10000)
            else:
                if not 0 <= args['x'] < session['image_width'] or not 0 <= args['y'] < session['image_height']:
                    raise ValueError('Coordinates are outside the observed browser screenshot')
                page.mouse.click(args['x'] / session['scale'], args['y'] / session['scale'])
        elif action == 'fill':
            if node is None:
                raise ValueError('fill requires an observed editable element ref')
            node.fill(args['text'], timeout=10000)
        elif action == 'press':
            (node if node else page.keyboard).press(args['key'])
        elif action == 'select':
            if node is None:
                raise ValueError('select requires an observed select ref')
            node.select_option(value=args['text'], timeout=10000)
        elif action == 'check':
            if node is None:
                raise ValueError('check requires an observed checkbox ref')
            node.set_checked(args.get('checked', True), timeout=10000)
        elif action == 'scroll':
            page.mouse.wheel(args.get('x', 0), args.get('y', 600))
        elif action == 'evaluate':
            value = page.evaluate(args['script'])
            result = self.observe(identifier)
            result['evaluation'] = json.loads(json.dumps(value)[:64000]) if len(json.dumps(value)) <= 64000 else {'omitted': 'evaluation exceeds 64 KiB'}
            return result
        elif action == 'upload':
            if node is None:
                raise ValueError('upload requires an observed file-input ref')
            node.set_input_files([str(self.file(path)) for path in args['files']], timeout=10000)
        elif action == 'dialog':
            session['dialog_policy'] = {'accept': args.get('accept', False), 'text': args.get('text', '')}
        elif action == 'new_tab':
            page = session['context'].new_page()
            session['page'] = page
            self.watch(session, page)
            page.goto(self.url(args.get('url', 'about:blank')), wait_until='domcontentloaded', timeout=20000)
        elif action == 'switch_tab':
            pages = session['context'].pages
            index = args['tab']
            if index < 0 or index >= len(pages):
                raise ValueError('Selected tab does not exist')
            session['page'] = pages[index]
            self.watch(session, pages[index])
        elif action in ('back', 'forward', 'reload'):
            operation = {'back': page.go_back, 'forward': page.go_forward, 'reload': page.reload}[action]
            operation(wait_until='domcontentloaded', timeout=20000)
        else:
            raise ValueError('Unsupported browser action')
        return self.observe(identifier)

    def watch(self, session, page):
        if getattr(page, '_peritus_watched', False):
            return
        page._peritus_watched = True
        page.set_default_timeout(10000)
        page.on('console', lambda event: self.event(session, {'kind': 'console', 'level': event.type, 'text': event.text[:2000]}))
        page.on('pageerror', lambda error: self.event(session, {'kind': 'pageerror', 'text': str(error)[:2000]}))
        page.on('dialog', lambda dialog: self.dialog(session, dialog))
        page.on('download', lambda download: self.download(session, download))

    def event(self, session, value):
        session['events'].append(value)
        session['events'] = session['events'][-20:]

    def dialog(self, session, dialog):
        policy = session.pop('dialog_policy', {'accept': False, 'text': ''})
        self.event(session, {'kind': 'dialog', 'type': dialog.type, 'message': dialog.message[:2000], 'accepted': policy['accept']})
        if policy['accept']:
            dialog.accept(policy['text'])
        else:
            dialog.dismiss()

    def download(self, session, download):
        path = session['directory'] / ('download-' + uuid.uuid4().hex)
        download.save_as(str(path))
        session['downloads'].append({'name': download.suggested_filename, 'path': str(path), 'bytes': path.stat().st_size, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})

    def observe(self, identifier, selector=None):
        session = self.sessions[identifier]
        for handle in session['refs'].values():
            try:
                handle.dispose()
            except Exception:
                pass
        session['refs'] = {}
        page = session['page']
        observation = uuid.uuid4().hex
        if page.is_closed():
            session['observation'] = observation
            return {'success': True, 'kind': 'browser', 'session': identifier,
                    'state': 'closed', 'owned': session['owned'], 'observation': observation,
                    'elements': [], 'image': None,
                    'recovery': 'Selected tab closed; switch to an existing tab, open a new tab, or close this session'}
        refs = {}
        elements = []
        omitted = len(page.frames) > 16
        for frame_index, frame in enumerate(page.frames[:16]):
            handles = frame.query_selector_all(selector or 'input,textarea,select,button,a[href],[role="button"],[role="checkbox"],[contenteditable="true"]')
            for handle in handles:
                if len(refs) >= 200:
                    omitted = True
                    handle.dispose()
                    continue
                box = handle.bounding_box()
                if box is None and handle.get_attribute('type') != 'file':
                    handle.dispose()
                    continue
                info = handle.evaluate('''e => ({tag:e.tagName.toLowerCase(), role:e.getAttribute('role'), name:e.getAttribute('aria-label') || (e.labels && Array.from(e.labels).map(l=>l.innerText).join(' ')) || e.innerText || e.getAttribute('placeholder') || e.getAttribute('name') || e.id, type:e.type || null, value:e.type==='password' ? '[password]' : (e.value || null), checked:typeof e.checked==='boolean' ? e.checked : null, disabled:!!e.disabled})''')
                ref = 'e' + str(len(refs) + 1)
                refs[ref] = handle
                info.update(ref=ref, frame=frame_index, box=box)
                if isinstance(info.get('name'), str):
                    info['name'] = info['name'][:256]
                if isinstance(info.get('value'), str):
                    info['value'] = info['value'][:512]
                elements.append(info)
        raw = session['directory'] / ('raw-' + observation + '.png')
        page.screenshot(path=str(raw), timeout=10000)
        image = self.screenshot(raw, session['directory'] / (observation + '.png'))
        session.update(refs=refs, observation=observation, scale=image['scale'], image_width=image['width'], image_height=image['height'])
        dialog = session.get('dialog')
        return {'success': True, 'kind': 'browser', 'session': identifier, 'observation': observation,
                'owned': session['owned'], 'url': page.url, 'title': page.title(),
                'text': page.locator('body').inner_text(timeout=3000)[:20000] if page.locator('body').count() else '',
                'elements': elements, 'elements_omitted': omitted, 'image': image, 'events': session['events'],
                'downloads': session['downloads'][-10:], 'dialog': {'type': dialog.type, 'message': dialog.message[:2000]} if dialog else None,
                'tabs': [{'index': index, 'url': tab.url} for index, tab in enumerate(session['context'].pages)]}

    def close(self, identifier):
        session = self.sessions.pop(identifier)
        if session['owned']:
            session['context'].close()
        else:
            session['browser'].close()  # CDP connection disconnects; existing browser stays alive.

    def shutdown(self):
        for identifier in list(self.sessions):
            try:
                self.close(identifier)
            except Exception:
                pass
        if self.reader:
            self.reader.close()
        if self.playwright:
            self.playwright.stop()
