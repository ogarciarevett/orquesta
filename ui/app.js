// Copyright (c) 2026 Lovecast Inc. SPDX-License-Identifier: MIT
const element = id => document.getElementById(id);
const recipient = 'session:s';
const preference = `orquesta:${recipient}:`;
const safety = element('safety');
let connected = true;
let signature = '';
let known = new Set();
let pending = false;
safety.checked = localStorage.getItem(preference + 'notifications') !== 'false';
safety.addEventListener('change', () => localStorage.setItem(preference + 'notifications', safety.checked));
function theme(value) {
  document.documentElement.dataset.theme = value;
  element('theme').textContent = `Theme: ${value === 'dark' ? 'Dark' : 'Light'}`;
}
theme(localStorage.getItem(preference + 'theme') || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'));
element('theme').addEventListener('click', () => {
  const next = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
  theme(next);
  localStorage.setItem(preference + 'theme', next);
});
async function ipc(command, fields = {}) {
  const response = await fetch('/ipc', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ version: 1, operation: { command, recipient, ...fields } }) });
  const data = await response.json();
  if (!response.ok || data.error) throw new Error(data.error?.message || 'Mailbox unavailable');
  return data.result;
}
function text(tag, content, className) {
  const node = document.createElement(tag);
  node.textContent = content;
  if (className) node.className = className;
  return node;
}
function render(messages) {
  const next = JSON.stringify(messages);
  if (signature === next) return;
  signature = next;
  const list = element('messages');
  const focused = list.contains(document.activeElement) ? document.activeElement.closest('[data-id]')?.dataset.id : null;
  const rows = messages.sort((a, b) => a.id - b.id).map(message => {
    const row = document.createElement('li');
    row.className = 'message';
    row.tabIndex = -1;
    row.dataset.id = message.id;
    const meta = text('div', '', 'meta');
    meta.append(text('span', `${message.envelope.from} · #${message.id}`), text('span', message.acknowledged ? 'Acknowledged' : message.read ? 'Read' : 'Unread', 'state'));
    row.append(meta, text('h3', message.envelope.subject), text('p', message.envelope.body, 'body'));
    const ack = text('button', message.acknowledged ? 'Acknowledged' : 'Acknowledge');
    ack.type = 'button';
    ack.dataset.ack = message.id;
    ack.disabled = message.acknowledged;
    ack.setAttribute('aria-label', `Acknowledge message ${message.id}`);
    ack.addEventListener('click', () => act('ack', { id: message.id }));
    row.append(ack);
    return row;
  });
  if (!rows.length) rows.push(text('li', 'Inbox is clear. Incoming messages will appear here, never in your draft.', 'empty'));
  list.replaceChildren(...rows);
  if (focused) {
    const button = list.querySelector(`[data-ack="${focused}"]`);
    if (button && document.activeElement === document.body) (button.disabled ? button.closest('li') : button).focus();
  }
  const unread = messages.filter(message => !message.read && !message.acknowledged).length;
  element('badge').textContent = unread;
  element('read-batch').disabled = !unread;
  const arrivals = messages.filter(message => !known.has(message.id)).length;
  if (safety.checked && arrivals) element('notice').textContent = `${arrivals} new inbox ${arrivals === 1 ? 'message' : 'messages'}. Your draft is unchanged.`;
  else element('notice').textContent = safety.checked ? 'Inbox is up to date.' : 'Inbox notifications are paused.';
  known = new Set(messages.map(message => message.id));
}
async function refresh() {
  if (!connected || pending) return;
  pending = true;
  try {
    const messages = await ipc('inbox');
    if (!connected) return;
    render(messages);
    element('status').textContent = 'Local mailbox · connected';
  }
  catch (error) { if (connected) element('status').textContent = error.message; }
  finally { pending = false; }
}
async function act(command, fields) {
  try { await ipc(command, fields); await refresh(); }
  catch (error) { element('status').textContent = error.message; }
}
element('read-batch').addEventListener('click', () => act('check', { peek: false }));
element('connection').addEventListener('click', async () => {
  connected = !connected;
  element('connection').textContent = connected ? 'Disconnect' : 'Reconnect';
  element('status').textContent = connected ? 'Reconnecting…' : 'Local mailbox · disconnected';
  if (connected) await act('check', { peek: false });
});
element('inbox-toggle').addEventListener('click', () => {
  const drawer = element('inbox');
  drawer.hidden = !drawer.hidden;
  element('inbox-toggle').setAttribute('aria-expanded', String(!drawer.hidden));
});
document.addEventListener('keydown', event => {
  if (event.key === 'Escape') {
    element('settings').open = false;
    if (element('inbox').contains(document.activeElement)) element('inbox-toggle').focus();
    element('inbox').hidden = true;
    element('inbox-toggle').setAttribute('aria-expanded', 'false');
  }
});
element('composer').addEventListener('submit', event => {
  event.preventDefault();
  element('submissions').value = Number(element('submissions').value) + 1;
});
refresh();
setInterval(refresh, 1000);
