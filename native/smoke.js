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
    const catalogBefore = await call({ kind: 'sidebar' }), add = document.querySelector('#add-project');
    add.focus(); add.click();
    for (let attempt = 0; add.getAttribute('aria-disabled') !== 'false' && attempt < 200; attempt++) await new Promise(resolve => setTimeout(resolve, 100));
    if (document.activeElement !== add || JSON.stringify(catalogBefore) !== JSON.stringify(await call({ kind: 'sidebar' }))) throw new Error('Picker cancellation changed focus or catalog');
    const chosen = await pick();
    if (!chosen.projects.some(project => project.label === 'orquesta-picked-folder' && project.kind === 'folder')) throw new Error('Native selected folder missing');
    const restart = await call({ kind: 'sidebar' });
    if (JSON.stringify(chosen) !== JSON.stringify(restart)) throw new Error('Native catalog not persisted');
    // Use the picker-produced catalog pair through the public named adapter.
    draft.focus(); draft.setSelectionRange(3, 14, 'backward');
    const composer = [draft.value, draft.selectionStart, draft.selectionEnd, draft.selectionDirection, document.activeElement];
    const unread = await window.orquestaNative('inbox');
    const initial = await window.orquestaNative('automations');
    const project = chosen.projects.find(project => project.label === 'orquesta-picked-folder');
    const definition = {
      name: 'Native review', prompt: ' \nReview λ\n  keep indentation\n',
      project_id: project.id, workspace_id: project.workspace_id,
      schedule: { kind: 'interval', every_seconds: 60, anchor_utc_seconds: 0 }
    };
    const fields = { revision: initial.revision, id: null, draft: definition };
    // Unknown top-level adapter input is omitted; the typed request remains narrow.
    const saved = await window.orquestaNative('save-automation', { ...fields, recipient: 'session:other', path: '/tmp' });
    if (saved.revision !== initial.revision + 1 || saved.definitions.length !== initial.definitions.length + 1) throw new Error('Native save failed');
    const record = saved.definitions.at(-1);
    const savedDraft = record.draft;
    if (!/^automation-\d+$/.test(record.id) ||
        ['name', 'prompt', 'project_id', 'workspace_id'].some(key => savedDraft[key] !== definition[key]) ||
        ['kind', 'every_seconds', 'anchor_utc_seconds'].some(key => savedDraft.schedule[key] !== definition.schedule[key])) throw new Error('Draft round trip differs');
    const persistedDrafts = await window.orquestaNative('automations');
    if (JSON.stringify(saved) !== JSON.stringify(persistedDrafts)) throw new Error('Native draft read differs');
    const reject = async (action, expected) => {
      try { await action(); } catch (error) {
        if (!(error instanceof Error) || expected && error.message !== expected) throw error;
        return;
      }
      throw new Error('Automation authority accepted');
    };
    await reject(() => window.orquestaNative('save-automation', fields), 'automations changed; reload snapshot before retrying');
    await reject(() => window.orquestaNative('save-automation', { revision: saved.revision, id: 'automation-999999', draft: definition }), 'unknown automation ID');
    await reject(() => window.orquestaNative('save-automation', { revision: saved.revision, id: null, draft: { ...definition, project_id: 'foreign' } }), 'unknown automation project/workspace pair');
    await reject(() => window.orquestaNative('save-automation', { revision: saved.revision, id: null, draft: { ...definition, env: {} } }));
    for (const kind of ['run', 'enable', 'delete']) await reject(() => window.orquestaNative(kind));
    for (const request of [
      { kind: 'automations', recipient: 'session:other' },
      { kind: 'save_automation', revision: saved.revision, id: null, draft: definition, cwd: '/tmp' },
      { kind: 'save_automation', revision: saved.revision, id: null, draft: { ...definition, schedule: { ...definition.schedule, kind: 'cron' } } }
    ]) {
      let rejected = false;
      try { await call(request); } catch { rejected = true; }
      if (!rejected) throw new Error('Raw automation request accepted');
    }
    if (JSON.stringify(await window.orquestaNative('automations')) !== JSON.stringify(saved) || JSON.stringify(await window.orquestaNative('inbox')) !== JSON.stringify(unread)) throw new Error('Rejected save changed drafts or unread mail');
    const composerAfter = [draft.value, draft.selectionStart, draft.selectionEnd, draft.selectionDirection, document.activeElement];
    if (draft !== document.querySelector('#draft') || composer.some((value, index) => value !== composerAfter[index]) || !submit.disabled || document.querySelector('#submissions').value !== '0') throw new Error('Automation calls changed composer');
    await invoke('smoke_complete', { success: true, detail: 'WKWebView named adapter -> Tauri -> Rust save/read with real picker IDs, CAS/authority rejection and composer/unread checks passed; same-mailbox core reopen verifies persistence, not OS app relaunch; no scheduling or execution proof.' });
  } catch (error) {
    await invoke('smoke_complete', { success: false, detail: String(error) });
  }
})();
