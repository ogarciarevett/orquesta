// Copyright (c) 2026 Lovecast Inc.
// Copyright (c) 2026 Omar Garcia
// SPDX-License-Identifier: MIT
(() => {
  const node = (tag, text = '', className = '') => Object.assign(document.createElement(tag), { textContent: text, className });
  const view = document.createElement('div');
  view.innerHTML = `<footer id="status-bar" aria-label="Workspace status"><div id="sb-providers"></div><div id="sb-segments"><button id="sb-items" popovertarget="sb-menu" aria-label="Status bar items">⋯</button></div></footer>
    <section id="sb-usage" popover aria-labelledby="sb-title"><header><h2 id="sb-title">Usage</h2><span id="sb-source"></span><button id="sb-refresh"><span class="sb-refresh-symbol" aria-hidden="true">↻</span> Refresh</button><button popovertarget="sb-usage" popovertargetaction="hide">Close</button></header><div id="sb-roster"></div><button data-detail="history">Usage details &amp; history</button><button data-detail="accounts">Manage accounts…</button></section>
    <section id="sb-menu" popover aria-label="Status bar items"><div class="sb-controls"></div><button id="sb-settings-open">Status bar settings</button></section>
    <dialog id="sb-settings" aria-labelledby="sb-settings-title"><form method="dialog"><button>Close</button></form><h2 id="sb-settings-title">Appearance › Window › Status bar</h2><div class="sb-controls"></div></dialog>
    <dialog id="sb-details" aria-labelledby="sb-detail-title"><form method="dialog"><button>Close</button></form><h2 id="sb-detail-title"></h2><div id="sb-detail-content"></div></dialog>
    <span id="sb-announcement" class="sb-sr" aria-live="polite"></span>`;
  document.body.append(view);
  const get = id => document.getElementById(id), bar = get('status-bar'), usage = get('sb-usage'), menu = get('sb-menu');
  const letters = { claude: 'C', codex: 'X', gemini: 'G', cursor: 'U', grok: 'R', kimi: 'K', minimax: 'M', antigravity: 'A', 'opencode-go': 'O', zcode: 'Z' };
  const labels = { resources: 'Resources', ports: 'Ports', hosts: 'Remote hosts', terminals: 'Terminals' };
  const providers = new Map(), roster = new Map(), controls = new Map(), previous = new Map(), busy = new Set();
  let state, polling = false;
  async function request(action) {
    const response = await fetch('/ipc', { signal: AbortSignal.timeout(10000), method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ version: 1, operation: { command: 'status-bar', action } }) });
    const body = await response.json();
    if (!response.ok || body.error) throw new Error(body.error?.message || 'Status unavailable');
    return body.result;
  }
  const max = provider => ['off', 'not_signed_in', 'loading', 'error_no_cache'].includes(provider.phase) ? -1 : Math.max(-1, ...provider.windows.map(window => window.percent_used ?? -1));
  const canRefresh = provider => state?.refresh_providers?.includes(provider.id);
  const percent = value => value == null ? 'unknown' : `${Math.round(value)}%`;
  const duration = seconds => seconds == null ? 'unknown' : `${Math.floor(seconds / 3600)} h ${Math.floor(seconds % 3600 / 60)} m`;
  const tone = value => value >= 80 ? 'urgent' : value >= 60 ? 'warning' : 'normal';
  const age = provider => provider.updated_at == null ? null : `${Math.max(0, Math.floor(Date.now() / 60000 - provider.updated_at / 60))} m`;
  function remaining(window, provider) {
    const reset = Date.parse(window.reset_at_iso || '');
    if (Number.isFinite(reset)) return Math.max(0, (reset - Date.now()) / 1000);
    if (window.reset_in_seconds == null) return null;
    return Math.max(0, window.reset_in_seconds - (provider.updated_at ? Date.now() / 1000 - provider.updated_at : 0));
  }
  function describe(provider) {
    const windows = provider.windows.map(window => `${percent(window.percent_used)}, resets in ${duration(remaining(window, provider))}`).join(' · ');
    const availability = { ready: 'on', off: 'off', not_signed_in: 'not signed in', loading: 'loading' }[provider.phase] || provider.phase.replaceAll('_', ' ');
    const updated = age(provider) ? `Updated ${age(provider)} ago` : 'Update time unavailable';
    return `${provider.name} usage ${windows || 'unknown'}, ${availability}. ${updated}${provider.phase === 'error_cached' ? '. Refresh failed — showing cached data' : ''}`;
  }
  function meter(window) {
    const track = node('span', '', 'sb-meter'), fill = node('i');
    fill.style.width = `${window.percent_used ?? 0}%`; track.append(fill); return track;
  }
  function refreshState() {
    const refreshing = busy.size > 0 || !!state?.snapshot.providers.some(provider => provider.phase === 'refreshing');
    get('sb-refresh').disabled = !state?.refresh_providers?.length || refreshing;
    get('sb-refresh').classList.toggle('sb-refreshing', refreshing);
  }
  async function refreshProvider(provider) {
    if (busy.has(provider.id) || !canRefresh(provider)) return;
    busy.add(provider.id);
    refreshState();
    try {
      const response = await fetch('/refresh', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id: provider.id, request_id: crypto.randomUUID() }),
        signal: AbortSignal.timeout(10000)
      });
      const result = await response.json();
      if (!response.ok || result.error) throw new Error('Usage refresh failed');
    } finally {
      busy.delete(provider.id);
      await poll();
      refreshState();
    }
  }
  get('sb-refresh').onclick = () => state?.snapshot.providers.forEach(provider => refreshProvider(provider).catch(() => {}));
  function details(kind) {
    const content = get('sb-detail-content'); content.replaceChildren();
    get('sb-detail-title').textContent = kind === 'accounts' ? 'Manage accounts…' : kind === 'history' ? 'Usage details & history' : labels[kind];
    if (kind === 'accounts') content.append(node('p', "Account controls aren’t available yet."));
    else if (kind === 'history') content.append(node('p', "Usage history isn’t available yet."));
    else content.append(node('p', kind === 'resources' ? `Host load: ${state.resources?.load ?? 'unknown'}` : Object.hasOwn(state.snapshot.inventory, kind) ? state.snapshot.inventory[kind].join(', ') || 'None' : 'Inventory is not connected.'));
    get('sb-details').showModal();
  }
  view.querySelectorAll('[data-detail]').forEach(button => button.onclick = () => details(button.dataset.detail));
  function configuration() {
    for (const container of view.querySelectorAll('.sb-controls')) {
      const entries = [...state.snapshot.providers.map(provider => [provider.id, provider.name]), ...Object.entries(labels)];
      for (const [key, name] of entries) {
        const id = `${container.parentElement.id}:${key}`;
        if (!controls.has(id)) {
          const label = node('label'), input = Object.assign(node('input'), { type: 'checkbox' }); input.dataset.key = key;
          input.onchange = async () => { input.disabled = true; try { render(await request({ action: 'visibility', key, visible: input.checked })); } finally { input.disabled = false; } };
          label.append(input, node('span', name)); container.insertBefore(label, container.querySelector('[data-mode]')?.parentElement || null); controls.set(id, input);
        }
        controls.get(id).checked = state.snapshot.visibility[key] !== false;
      }
      if (!container.querySelector('[data-mode]')) for (const [mode, name, description] of [['compact', 'Compact', 'Condensed usage: one summary per provider'], ['detailed', 'Detailed', 'Full usage with bars, labels, and percentages']]) {
        const label = node('label'), input = Object.assign(node('input'), { type: 'radio', name: `${container.parentElement.id}-mode`, value: mode }); input.dataset.mode = mode;
        input.onchange = async () => render(await request({ action: 'display', mode })); label.append(input, node('span', name), node('small', description)); container.append(label);
      }
      container.querySelectorAll('[data-mode]').forEach(input => input.checked = input.value === state.snapshot.display_mode);
    }
  }
  function providerView(provider) {
    let chip = providers.get(provider.id), row = roster.get(provider.id);
    if (!chip) {
      chip = node('button', '', 'sb-provider'); chip.dataset.id = provider.id; chip.popoverTargetElement = usage;
      get('sb-providers').insertBefore(chip, get('sb-overflow')); providers.set(provider.id, chip);
      row = node('section', '', 'sb-row'); row.dataset.id = provider.id; get('sb-roster').append(row); roster.set(provider.id, row);
      row.append(node('strong', provider.name), node('span', '', 'sb-plan'), node('div', '', 'sb-metrics'));
    }
    const phase = provider.phase === 'loading' && !canRefresh(provider) ? 'off' : provider.phase;
    chip.hidden = state.snapshot.visibility[provider.id] === false;
    chip.title = chip.ariaLabel = describe({ ...provider, phase });
    chip.dataset.tone = tone(max(provider)); chip.dataset.phase = phase;
    chip.replaceChildren(node('span', letters[provider.id] || provider.name[0], 'sb-letter'));
    const empty = { loading: '···', off: '--', not_signed_in: 'Sign in', error_no_cache: 'Refresh failed' }[phase];
    const tightest = provider.windows.reduce((a, b) => !a || (b.percent_used ?? -1) > (a.percent_used ?? -1) ? b : a, null);
    const chipWindows = state.snapshot.display_mode === 'compact' && tightest ? [tightest] : provider.windows;
    if (!empty && tightest) chip.append(meter(tightest));
    if (empty) chip.append(node('span', empty, 'sb-state'));
    else for (const window of chipWindows) {
      const part = node('span', '', `sb-window${window === tightest ? ' sb-tight' : ''}`);
      const value = node('span', percent(window.percent_used), 'sb-number'); value.dataset.tone = tone(window.percent_used);
      part.append(value, node('span', window.label, 'sb-window-label')); chip.append(part);
    }
    if (['stale', 'error_cached', 'error_no_cache'].includes(phase)) chip.append(node('span', '⚠︎', 'sb-warning'));
    const indicator = node('span', phase === 'refreshing' ? '↻' : '', 'sb-indicator');
    indicator.classList.toggle('sb-refreshing', phase === 'refreshing');
    chip.append(indicator);
    const signature = JSON.stringify({ provider, phase });
    if (row.signature !== signature) {
      row.signature = signature;
      row.querySelector('strong').textContent = provider.name;
      row.querySelector('.sb-plan').textContent = provider.plan || 'Plan unavailable';
      const metrics = row.querySelector('.sb-metrics'); metrics.replaceChildren();
      if (empty) metrics.append(node('p', phase === 'not_signed_in' ? 'Sign in to see usage' : empty));
      for (const window of empty ? [] : provider.windows) {
        const line = node('p', '', 'sb-window'), value = node('span', percent(window.percent_used), 'sb-number'); value.dataset.tone = tone(window.percent_used);
        const reset = node('span', `resets in ${duration(remaining(window, provider))}`); reset.title = window.reset_at_iso || 'Reset time unknown';
        reset.dataset.reset = JSON.stringify({ window, updated_at: provider.updated_at });
        line.append(meter(window), value, node('span', window.label), reset); metrics.append(line);
      }
      if (['stale', 'error_cached'].includes(phase)) metrics.append(node('p', phase === 'stale' ? (age(provider) ? `⚠︎ Updated ${age(provider)} ago` : '⚠︎ Update time unavailable') : '⚠︎ Refresh failed — showing cached data'));
      row.querySelector('button')?.remove();
      if (phase === 'not_signed_in') {
        const signIn = node('button', 'Sign in'); signIn.onclick = () => details('accounts'); row.append(signIn);
      }
    }
    const before = previous.get(provider.id), current = max(provider);
    if (before != null && before < 80 && current >= 80) get('sb-announcement').textContent = `${provider.name} usage ${percent(current)} is urgent.`;
    if (current >= 0) previous.set(provider.id, current);
  }
  function icon(key) {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg'), path = document.createElementNS(svg.namespaceURI, 'path');
    svg.setAttribute('viewBox', '0 0 24 24'); svg.setAttribute('aria-hidden', 'true');
    path.setAttribute('d', { resources: 'M2 12h4l3-8 6 16 3-8h4', ports: 'M8 2v6m8-6v6M6 8h12v3a6 6 0 0 1-12 0zm6 9v5', hosts: 'M3 3h18v7H3zm0 11h18v7H3zM6 6h1m-1 11h1', terminals: 'M3 4h18v16H3zm3 4 4 4-4 4m7 0h4' }[key]);
    svg.append(path); return svg;
  }
  const overflow = node('button', '', 'sb-overflow'); overflow.id = 'sb-overflow'; overflow.popoverTargetElement = usage;
  get('sb-providers').append(overflow);
  function fit() {
    if (!state) return;
    const visible = state.snapshot.providers.filter(provider => state.snapshot.visibility[provider.id] !== false);
    visible.forEach(provider => providers.get(provider.id).hidden = false); overflow.hidden = true;
    bar.dataset.density = state.snapshot.display_mode;
    const fits = () => get('sb-providers').scrollWidth <= get('sb-providers').clientWidth;
    if (!fits()) bar.dataset.density = 'compact';
    if (!fits()) bar.dataset.density = 'tight';
    const hidden = [];
    const highest = [...visible].sort((a, b) => max(b) - max(a))[0];
    const candidates = [...visible].reverse().sort((a, b) => (max(a) >= 80) - (max(b) >= 80));
    for (const provider of candidates) {
      if (innerWidth <= 500 ? provider === highest : fits()) continue;
      providers.get(provider.id).hidden = true; hidden.push(provider);
      overflow.hidden = false; overflow.replaceChildren(node('span', `+${hidden.length}`));
    }
    overflow.title = `Also: ${hidden.map(provider => `${provider.name} ${percent(max(provider) < 0 ? null : max(provider))}`).join(', ')}`;
    overflow.dataset.tone = tone(Math.max(-1, ...hidden.map(max))); overflow.ariaLabel = overflow.title;
    if (!fits() || innerWidth <= 500) bar.dataset.density = 'icons';
  }
  function countdowns() {
    for (const reset of view.querySelectorAll('[data-reset]')) {
      const data = JSON.parse(reset.dataset.reset);
      reset.textContent = `resets in ${duration(remaining(data.window, data))}`;
    }
  }
  function render(next) {
    state = next; configuration();
    get('sb-source').textContent = state.refresh_providers?.length ? 'Fixture data' : 'Refresh unavailable';
    get('sb-refresh').title = state.refresh_providers?.length ? 'Refresh usage' : 'Usage refresh unavailable on this host'; state.snapshot.providers.forEach(providerView);
    for (const [key, name] of Object.entries(labels)) {
      let button = get(`sb-${key}`);
      if (!button) { button = node('button'); button.id = `sb-${key}`; button.onclick = () => details(key); get('sb-segments').insertBefore(button, get('sb-items')); }
      const known = key === 'resources' ? state.resources?.load != null : Object.hasOwn(state.snapshot.inventory, key);
      const count = known ? (key === 'resources' ? state.resources.load.toFixed(1) : state.counts[key]) : '--';
      button.replaceChildren(icon(key), node('span', `${count}`), node('span', key === 'hosts' && known ? 'connected' : key === 'resources' ? 'load' : key, 'sb-segment-name'));
      button.ariaLabel = !known ? `${name} unavailable` : key === 'resources' ? `Host load, ${count === '--' ? 'unknown' : count}` : key === 'ports' ? `Ports, ${count} workspace ports` : `${count} ${key === 'hosts' ? 'connected remote hosts' : name.toLowerCase()}`; button.title = button.ariaLabel; button.hidden = state.snapshot.visibility[key] === false;
    }
    countdowns(); refreshState(); fit();
  }
  async function poll() { if (polling) return; polling = true; try { render(await request({ action: 'snapshot' })); } catch { bar.title = 'Status unavailable'; } finally { polling = false; } }
  const openMenu = event => { event.preventDefault(); menu.showPopover({ source: get('sb-items') }); };
  bar.oncontextmenu = openMenu;
  bar.onkeydown = event => { if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) openMenu(event); };
  get('sb-settings-open').onclick = () => get('sb-settings').showModal();
  const settingsLink = node('a', 'Status bar settings'); settingsLink.href = '#sb-settings'; settingsLink.onclick = event => { event.preventDefault(); get('sb-settings').showModal(); }; get('settingsdialog').append(settingsLink);
  new ResizeObserver(fit).observe(bar); poll(); setInterval(() => { countdowns(); poll(); }, 1500);
})();
