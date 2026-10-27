// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
(async () => {
  const invoke = window.__TAURI__.core.invoke;
  const call = request => request.kind === 'ack' ? window.orquestaNative('ack', request) : invoke('desktop_request', { request });
  try {
    for (let attempt = 0; document.querySelector('#status').textContent !== 'Connected' && attempt < 50; attempt++)
      await new Promise(resolve => setTimeout(resolve, 100));
    if (document.querySelector('#inbox').open || document.querySelector('#status').textContent !== 'Connected') throw new Error('Native inbox UI did not initialize');
    const draft = document.querySelector('#draft');
    draft.value = 'First line\nSecond line'; draft.focus(); draft.setSelectionRange(3, 14, 'backward');
    const before = [draft.value, draft.selectionStart, draft.selectionEnd, draft.selectionDirection, document.activeElement];
    await call({ kind: 'inbox' }); await call({ kind: 'read' }); await call({ kind: 'reconnect' });
    const status = await call({ kind: 'status' });
    if (status.snapshot.providers.length !== 10 || status.refresh_providers.length) throw new Error('Invalid native provider state');
    await call({ kind: 'display', mode: 'detailed' });
    await call({ kind: 'visibility', key: 'ports', visible: false });
    const persisted = await call({ kind: 'status' });
    if (persisted.snapshot.display_mode !== 'detailed' || persisted.snapshot.visibility.ports) throw new Error('Settings not persisted');
    for (const request of [{ kind: 'add_project', path: '/tmp' }, { kind: 'sidebar', host_url: 'https://example.com' }, { kind: 'inbox', recipient: 'session:other' }, { kind: 'refresh' }, { kind: 'ack', id: 999999 }]) {
      let rejected = false;
      try { await call(request); } catch (error) {
        rejected = request.kind !== 'ack' || error instanceof Error && error.message === 'message not in recipient inbox';
      }
      if (!rejected) throw new Error('Authority or unavailable operation accepted');
    }
    const submit = document.querySelector('#composer button[type=submit]');
    submit.scrollIntoView({ block: 'end' });
    if (submit.getBoundingClientRect().bottom > document.querySelector('#status-bar').getBoundingClientRect().top) throw new Error('Status bar obscures Submit');
    await new Promise(resolve => setTimeout(resolve, 2200));
    const after = [draft.value, draft.selectionStart, draft.selectionEnd, draft.selectionDirection, document.activeElement];
    if (draft !== document.querySelector('#draft') || before.some((value, index) => value !== after[index]) || document.querySelector('#submissions').value !== '0') throw new Error('Draft changed');
    const pick = () => Promise.race([call({ kind: 'add_project' }), new Promise((_, reject) => setTimeout(() => reject(new Error('Native picker automation timed out; inspect picker-driver.log')), 20000))]);
    if (await pick() !== null) throw new Error('Cancel changed catalog');
    const chosen = await pick();
    if (!chosen.projects.some(project => project.label === 'orquesta-picked-folder' && project.kind === 'folder')) throw new Error('Native selected folder missing');
    const restart = await call({ kind: 'sidebar' });
    if (JSON.stringify(chosen) !== JSON.stringify(restart)) throw new Error('Native catalog not persisted');
    await invoke('smoke_complete', { success: true, detail: 'WKWebView invoke, native picker cancel/selection, persistence, rejection and draft checks passed; no live mail delivery or PTY proof.' });
  } catch (error) {
    await invoke('smoke_complete', { success: false, detail: String(error) });
  }
})();
