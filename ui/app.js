// Copyright (c) 2026 Lovecast Inc.
// Copyright (c) 2026 Omar Garcia
// SPDX-License-Identifier: MIT
const element = id => document.getElementById(id);
const recipient = 'session:s';
const preference = `orquesta:host:${location.host}:`;
const safety = element('safety'), inbox = element('inbox'), settings = element('settingsdialog');
const mobile = matchMedia('(max-width: 1300px)');
const rows = new Map();
const closeReturns = [];
let connected = true, pending = false, generation = 0;
safety.checked = localStorage.getItem(preference + 'notifications') !== 'false';
safety.onchange = () => { localStorage.setItem(preference + 'notifications', safety.checked); if (!safety.checked) element('notice').textContent = ''; };
function theme(value) {
  document.documentElement.dataset.theme = value;
  element('theme').textContent = `${window.orquestaNative ? 'Theme' : 'Dev: Theme'} ${value === 'dark' ? 'Dark' : 'Light'}`;
}
theme(localStorage.getItem(preference + 'theme') || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'));
element('theme').onclick = () => {
  const next = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
  theme(next);
  localStorage.setItem(preference + 'theme', next);
};
// Close events are queued; retain their focus policy until each event is handled.
function closeInbox(returnFocus) {
  if (!inbox.open) return;
  const closure = { returnFocus };
  closeReturns.push(closure);
  inbox.close();
  closure.focusAfterClose = document.activeElement;
}
function layout() {
  const focused = document.activeElement;
  closeInbox(inbox.contains(focused));
  if (!mobile.matches) inbox.open = true;
  element('inbox-toggle').setAttribute('aria-expanded', String(inbox.open));
  if (focused === element('draft') && document.activeElement !== focused) focused.focus({ preventScroll: true });
}
layout();
mobile.onchange = layout;
element('inbox-toggle').onclick = () => { if (inbox.open) closeInbox(true); else if (mobile.matches) inbox.showModal(); else inbox.show(); element('inbox-toggle').setAttribute('aria-expanded', String(inbox.open)); };
element('inbox-close').onclick = () => closeInbox(true);
inbox.oncancel = event => { event.preventDefault(); closeInbox(true); };
inbox.onclose = () => {
  const closure = closeReturns.shift();
  element('inbox-toggle').setAttribute('aria-expanded', String(inbox.open));
  const focused = document.activeElement;
  if ((closure?.returnFocus ?? true) && !inbox.open && (!closure || focused === closure.focusAfterClose)) element('inbox-toggle').focus();
};
document.querySelectorAll('a[href="#settingsdialog"]').forEach(link => { link.onclick = event => { event.preventDefault(); settings.showModal(); }; });
element('settings-close').onclick = () => settings.close();
async function ipc(command, fields = {}) {
  if (window.orquestaNative) return window.orquestaNative(command, fields);
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
function times() {
  document.querySelectorAll('[data-created]').forEach(node => {
    const seconds = Number(node.dataset.created), elapsed = Math.max(0, Math.floor(Date.now() / 1000 - seconds));
    node.textContent = !seconds ? 'Time unavailable' : elapsed < 60 ? 'just now' : `${Math.floor(elapsed / 60)} m ago`;
  });
}
function render(messages) {
  let arrivals = 0;
  for (const message of messages.sort((a, b) => a.id - b.id)) {
    let row = rows.get(message.id);
    if (!row) { row = text('li', '', 'message'); row.dataset.id = message.id; row.tabIndex = -1; rows.set(message.id, row); element('messages').append(row); arrivals += Number(!message.read && !message.acknowledged); }
    if (row.signature === JSON.stringify(message)) continue;
    row.signature = JSON.stringify(message);
    const focused = row.contains(document.activeElement);
    row.className = `message${!message.read && !message.acknowledged ? ' unread' : ''}`;
    const state = text('span', message.acknowledged ? 'Acknowledged' : message.read ? 'Read' : 'Unread', 'state');
    const meta = text('div', '', 'meta'), date = text('time', '');
    date.dataset.created = message.created_at || 0;
    if (message.created_at) date.dateTime = date.title = new Date(message.created_at * 1000).toISOString();
    const sender = { run: 'Coordinator', dispatch: 'Worker', session: 'Terminal' }[message.envelope.from.split(':')[0]] || 'Sender';
    meta.append(text('strong', sender), text('span', message.envelope.from), date);
    row.replaceChildren(state, text('h3', message.envelope.subject), meta, text('p', message.envelope.body, 'body'));
    if (!message.acknowledged) {
      const ack = text('button', 'Acknowledge');
      ack.type = 'button'; ack.dataset.ack = message.id; ack.setAttribute('aria-label', `Acknowledge message ${message.id}`);
      ack.onclick = () => act('ack', { id: message.id }); row.append(ack);
    }
    if (focused && document.activeElement === document.body) (row.querySelector('button') || row).focus();
  }
  const unread = messages.filter(message => !message.read && !message.acknowledged).length;
  element('badge').textContent = unread; element('badge').hidden = !unread;
  element('inbox-toggle').setAttribute('aria-label', unread ? `Inbox, ${unread} unread` : 'Inbox');
  element('read-batch').disabled = !unread; element('empty').hidden = !!messages.length;
  element('notice').textContent = unread && safety.checked && arrivals ? `${arrivals} new inbox ${arrivals === 1 ? 'message' : 'messages'}.` : '';
  times();
}
function failure(error) { element('status').textContent = `Error: ${error.message}`; element('retry').hidden = false; }
async function refresh() {
  if (!connected || pending) return;
  pending = true;
  const current = generation;
  try {
    const messages = await ipc('inbox');
    if (!connected || generation !== current) return;
    render(messages); element('status').textContent = 'Connected'; element('retry').hidden = true;
    const date = new Date(); element('last-good').dateTime = element('last-good').title = date.toISOString(); element('last-good').textContent = `synced ${date.toLocaleTimeString()}`;
  } catch (error) { if (connected && generation === current) failure(error); }
  finally { pending = false; }
}
async function act(command, fields) {
  const current = generation;
  try { await ipc(command, fields); if (connected && generation === current) await refresh(); }
  catch (error) { if (connected && generation === current) failure(error); }
}
async function reconnect() {
  connected = true; generation++;
  element('connection').textContent = 'Disconnect'; element('status').textContent = 'Reconnecting…'; element('retry').hidden = true;
  await act('check', { peek: false });
}
element('read-batch').onclick = () => act('check', { peek: false });
element('retry').onclick = reconnect;
element('connection').onclick = () => {
  if (!connected) return reconnect();
  connected = false; generation++; element('connection').textContent = 'Reconnect'; element('status').textContent = 'Disconnected'; element('retry').hidden = true;
};
element('composer').onsubmit = event => { event.preventDefault(); if (window.orquestaNative) return; element('submissions').value = Number(element('submissions').value) + 1; };
refresh();
setInterval(() => { times(); refresh(); }, 1000);
