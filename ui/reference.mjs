// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
import { writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { spawn } from 'node:child_process';
const root = process.env.ORCA_REFERENCE_ROOT;
if (!root) throw new Error('ORCA_REFERENCE_ROOT must name the pinned Orca checkout');
const fixture = join(root, 'src/renderer/src/components/status-bar/orquesta-reference.fixture.tsx');
writeFileSync(fixture, `// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
import React, { useState } from 'react';
import { defineFixture } from '../../component-fixtures/component-fixture';
import { ProviderSegment } from './StatusBarProviderSegment';
import { UsageRosterPanel } from './UsageRosterPanel';
import { StatusBarVisibilityMenu } from './StatusBarVisibilityMenu';
import type { StatusBarController } from './use-status-bar-controller';
import type { ProviderRateLimits } from '../../../../shared/rate-limit-types';
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from '../ui/dropdown-menu';
const query = new URLSearchParams(location.search), view = query.get('view') || 'bar';
const state = query.get('state') || 'ok';
const noop = () => {};
if (!query.has('providers')) throw new Error('An explicit reference fixture is required');
const providers: ProviderRateLimits[] = JSON.parse(query.get('providers')!);
function Reference() {
  const [open, setOpen] = useState(view === 'roster'), [menu, setMenu] = useState(view === 'visibility');
  const [mode, setMode] = useState<'verbose' | 'compact'>(query.get('mode') === 'detailed' ? 'verbose' : 'compact'), [items, setItems] = useState<string[]>(JSON.parse(query.get('items') || '["claude","codex","gemini"]'));
  // SAFETY: this fixture supplies every field read by the visibility component, without invoking its runtime controller.
  const controller = { detectedAgentIds: null, menuOpen: menu, menuPoint: { x: 12, y: 0 }, recordFeatureInteraction: noop, setMenuOpen: setMenu, statusBarItems: items, toggleStatusBarItem: (item: string) => setItems(items.includes(item) ? items.filter(i => i !== item) : [...items, item]) } as StatusBarController;
  return <div className="fixed inset-x-0 bottom-0 flex items-center h-6 min-h-[24px] px-3 gap-4 overflow-clip whitespace-nowrap border-t border-border bg-[var(--bg-titlebar,var(--card))] text-xs select-none" onContextMenu={event => { event.preventDefault(); setMenu(true); }}>
    <DropdownMenu open={open} onOpenChange={setOpen} modal={false}>
      <DropdownMenuTrigger asChild><button type="button" aria-label="Usage" className="inline-flex items-center gap-3 rounded px-1 py-0.5 hover:bg-accent/70">{providers.map(p => <ProviderSegment key={p.provider} p={p} compact={false} mode={mode} display="used" />)}</button></DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="start" sideOffset={8} collisionPadding={{ top: 8, bottom: 32, left: 8, right: 8 }} className="w-[360px] p-0">
        <UsageRosterPanel providers={providers} display="used" statusBarUsageMode={mode} onStatusBarUsageModeChange={setMode} isRefreshing={state === 'fetching'} onRefresh={noop} onOpenProvider={noop} onSignIn={noop} canSignIn={() => true} onManageAccounts={noop} onUsageDetails={noop} />
      </DropdownMenuContent>
    </DropdownMenu>
    <StatusBarVisibilityMenu controller={controller} />
  </div>;
}
export const reference = defineFixture({ render: () => <Reference /> });
`);
const server = spawn(process.execPath, [join(root, 'node_modules/vite/bin/vite.js'), '--config', 'config/vite.fixtures.config.ts', '--host', '127.0.0.1', '--port', '5179', '--strictPort'], { cwd: root, stdio: 'inherit' });
const cleanup = () => { server.kill('SIGTERM'); rmSync(fixture, { force: true }); };
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => { cleanup(); process.exit(); });
server.on('exit', code => { rmSync(fixture, { force: true }); process.exit(code || 0); });
