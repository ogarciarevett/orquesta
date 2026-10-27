// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
if (window.__TAURI__) {
  window.orquestaNative = (command, fields = {}) => {
    let request;
    if (command === 'status-bar') {
      const { action, key, visible, mode } = fields;
      if (action === 'snapshot') request = { kind: 'status' };
      else if (action === 'visibility') request = { kind: 'visibility', key, visible };
      else if (action === 'display') request = { kind: 'display', mode };
      else return Promise.reject(new Error('Unsupported native status action'));
    } else if (command === 'inbox') request = { kind: 'inbox' };
    else if (command === 'check') request = { kind: 'read' };
    else if (command === 'ack') request = { kind: 'ack', id: fields.id };
    else return Promise.reject(new Error('Unsupported native mailbox action'));
    return window.__TAURI__.core.invoke('desktop_request', { request });
  };
  addEventListener('DOMContentLoaded', () => {
    document.querySelector('#theme').textContent = 'Theme';
    document.querySelector('#composer button[type=submit]').disabled = true;
    document.querySelector('#draft-help').textContent = 'Draft preview. Terminal submission is not connected. Incoming mail stays in the inbox.';
  });
}
