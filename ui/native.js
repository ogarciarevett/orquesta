// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
if (window.__TAURI__) {
  const kinds = { snapshot: 'status', visibility: 'visibility', display: 'display' };
  const requests = {
    sidebar: () => ({ kind: 'sidebar' }),
    'add-project': () => ({ kind: 'add_project' }),
    'sidebar-preference': ({ revision, patch }) => ({ kind: 'sidebar_preference', revision, patch }),
    inbox: () => ({ kind: 'inbox' }),
    check: () => ({ kind: 'read' }),
    ack: ({ id }) => ({ kind: 'ack', id }),
    'status-bar': ({ action, ...fields }) => ({ kind: kinds[action], ...fields })
  };
  window.orquestaNative = (command, fields = {}) =>
    window.__TAURI__.core.invoke('desktop_request', { request: requests[command]?.(fields) }).catch(error => {
      throw error instanceof Error ? error : new Error(typeof error === 'string' && error ? error : 'Mailbox unavailable');
    });
  document.querySelector('#composer button[type=submit]').disabled = true;
  document.querySelector('#draft-help').textContent = 'Draft preview. Terminal submission is not connected. Incoming mail stays in the inbox.';
}
