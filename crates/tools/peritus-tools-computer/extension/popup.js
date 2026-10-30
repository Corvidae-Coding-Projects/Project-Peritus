const state = document.querySelector('#state');
const toggle = document.querySelector('#toggle');

async function refresh() {
  const result = await chrome.runtime.sendMessage({kind: 'popup-status'});
  const connected = result.connected && !result.paused;
  state.className = connected ? 'ok' : 'off';
  state.textContent = result.paused
    ? 'Paused. Peritus cannot control Chrome.'
    : result.connected
      ? `Connected. ${result.sessions} selected session(s).`
      : `Waiting for the local Peritus native host.${result.bridgeError ? ` ${result.bridgeError}` : ''}`;
  toggle.textContent = result.paused ? 'Resume' : 'Pause';
}

toggle.addEventListener('click', async () => {
  const current = await chrome.runtime.sendMessage({kind: 'popup-status'});
  await chrome.runtime.sendMessage({kind: 'popup-pause', paused: !current.paused});
  await refresh();
});

refresh().catch(error => {
  state.className = 'off';
  state.textContent = `Status unavailable: ${error.message}`;
});

for (const [id, kind] of [['share','popup-share'],['revoke','popup-revoke']]) {
  document.querySelector('#'+id).addEventListener('click', async () => {
    try {
      const result = await chrome.runtime.sendMessage({kind});
      if (!result.success) throw new Error(result.error);
      await refresh();
      state.textContent += result.shared ? ` Shared ${result.origin} for reading.` : ' Tab access revoked.';
    } catch (error) { state.textContent = error.message; state.className = 'off'; }
  });
}
