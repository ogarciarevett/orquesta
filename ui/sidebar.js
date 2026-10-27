// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
(() => {
  const get = id => document.getElementById(id);
  const panel = get('projects'), list = get('project-list'), toggle = get('projects-toggle');
  const narrow = matchMedia('(max-width: 920px)');
  let snapshot, busy = false;
  function sheet(open, restore = false) {
    if (panel.open) panel.close();
    panel.hidden = narrow.matches && !open;
    if (narrow.matches && open) panel.showModal();
    else if (!narrow.matches) panel.open = true;
    toggle.setAttribute('aria-expanded', String(!panel.hidden));
    if (restore) toggle.focus();
  }
  function layout() {
    const hiddenFocus = narrow.matches && panel.contains(document.activeElement);
    sheet(false, hiddenFocus);
  }
  toggle.onclick = () => { sheet(panel.hidden); if (!panel.hidden) get('add-project').focus(); };
  get('projects-close').onclick = () => sheet(false, true);
  panel.oncancel = event => { event.preventDefault(); sheet(false, true); };
  narrow.onchange = layout;
  layout();
  function button(label, id, action) {
    const node = document.createElement('button');
    node.type = 'button'; node.textContent = label; node.dataset.id = id;
    node.onclick = action;
    return node;
  }
  function group(label, id, parent) {
    const expanded = !snapshot.collapsed.includes(id);
    const heading = button(label, id, () => preference({ action: 'collapse', id, collapsed: expanded }));
    heading.className = 'project-group'; heading.setAttribute('aria-expanded', String(expanded));
    heading.onkeydown = event => {
      if (['ArrowLeft', 'ArrowRight'].includes(event.key)) {
        event.preventDefault(); preference({ action: 'collapse', id, collapsed: event.key === 'ArrowLeft' });
      }
    };
    parent.append(heading);
    const children = document.createElement('div'); children.hidden = !expanded; parent.append(children);
    return children;
  }
  function render() {
    const focused = list.contains(document.activeElement) ? document.activeElement.dataset.id : null;
    list.replaceChildren();
    if (!snapshot.projects.length) { get('project-status').textContent = 'Add an existing local folder to get started.'; return; }
    get('project-status').textContent = '';
    toggle.textContent = snapshot.projects.find(project => project.workspace_id === snapshot.selected)?.label || 'Projects';
    const host = group(`This Mac · ${snapshot.projects.length} folders`, 'local', list);
    for (const project of snapshot.projects) {
      const children = group(`${project.label} — This Mac`, project.id, host);
      const options = document.createElement('div'); options.setAttribute('role', 'listbox');
      options.setAttribute('aria-label', `${project.label} folders`);
      const row = button(`${project.label} · Primary folder`, project.workspace_id, () => preference({ action: 'select', workspace_id: project.workspace_id }));
      row.className = 'project-folder'; row.setAttribute('role', 'option');
      const selected = snapshot.selected === project.workspace_id;
      row.setAttribute('aria-selected', String(selected));
      if (selected) row.setAttribute('aria-current', 'true');
      row.onkeydown = event => {
        if (!['ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown'].includes(event.key)) return;
        event.preventDefault();
        const visible = [...list.querySelectorAll('[role=option]')].filter(node => node.checkVisibility());
        const index = visible.indexOf(row);
        const next = ['Home', 'PageUp'].includes(event.key) ? 0 : ['End', 'PageDown'].includes(event.key) ? visible.length - 1 : index + (event.key === 'ArrowUp' ? -1 : 1);
        visible[Math.max(0, Math.min(next, visible.length - 1))]?.focus();
      };
      options.append(row); children.append(options);
    }
    if (focused) [...list.querySelectorAll('button')].find(node => node.dataset.id === focused)?.focus({ preventScroll: true });
  }
  async function request(command, fields) {
    if (busy) return;
    busy = true; get('add-project').setAttribute('aria-disabled', 'true'); get('projects-retry').hidden = true;
    try {
      if (!window.orquestaNative) throw new Error('Open the desktop app to add local projects.');
      const next = await window.orquestaNative(command, fields);
      if (next && (!snapshot || next.revision >= snapshot.revision)) { snapshot = next; render(); }
      if (command === 'sidebar-preference' && fields.patch.action === 'select' && narrow.matches) sheet(false, true);
    } catch (error) {
      get('project-status').textContent = error.message; get('projects-retry').hidden = false;
    } finally { busy = false; get('add-project').setAttribute('aria-disabled', String(!window.orquestaNative)); }
  }
  const preference = patch => request('sidebar-preference', { revision: snapshot.revision, patch });
  get('add-project').onclick = () => request('add-project');
  get('projects-retry').onclick = () => request('sidebar');
  request('sidebar');
})();
