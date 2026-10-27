# Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
"""Projection fixtures exercise UI only; they do not prove native picker operation."""
import unittest
import os
import subprocess
from urllib.parse import urlencode
import test_focus
from test_ui import browser, evaluate, SHOTS, ipc


class SidebarUI(unittest.TestCase):
    setUp = test_focus.FocusUI.setUp
    close_host = test_focus.FocusUI.close_host
    draft_state = test_focus.FocusUI.draft_state
    prepare_draft = test_focus.FocusUI.prepare_draft
    def test_projection_selection_collapse_and_draft(self):
        before = self.prepare_draft()
        ipc(dict(command='send', envelope=dict(request_id='sidebar-unread', to='session:s',
            **{'from': 'run:r'}, kind='status', subject='Keep unread', body='Folder selection is not mail reading', thread=None)))
        reference = subprocess.Popen(['node', 'ui/reference.mjs']) if os.environ.get('ORCA_REFERENCE_ROOT') else None
        if reference:
            self.addCleanup(lambda: (reference.terminate(), reference.wait()))
        evaluate("""window.sidebarFixture = {revision: 1, selected: null, collapsed: [], projects: [
          {id: 'project-7', workspace_id: 'folder-9', label: '日本語 project', host_id: 'local', kind: 'folder'}]};
          window.sidebarRequests = [];
          window.orquestaNative = async (command, fields) => {
            window.sidebarRequests.push({command, fields});
            if (command === 'add-project') return new Promise(resolve => { window.cancelPicker = resolve; });
            if (!['sidebar', 'sidebar-preference'].includes(command)) {
              const operation = command === 'status-bar' ? {command, action: fields} : {command, recipient: 'session:s', ...fields};
              return (await (await fetch('/ipc', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({version: 1, operation})})).json()).result;
            }
            if (command === 'sidebar-preference') {
              const patch = fields.patch;
              if (patch.action === 'select') window.sidebarFixture.selected = patch.workspace_id;
              else window.sidebarFixture.collapsed = patch.collapsed ? [patch.id] : [];
              window.sidebarFixture.revision++;
            }
            return structuredClone(window.sidebarFixture);
          };
          document.querySelector('#projects-retry').click();""")
        browser('wait', '--fn', "!!document.querySelector('[data-id=folder-9]')")
        self.assertEqual(self.draft_state(), before)
        browser('click', '#add-project')
        self.assertEqual(evaluate('document.activeElement.id'), 'add-project')
        evaluate('window.cancelPicker(null)')
        browser('wait', '--fn', "document.querySelector('#add-project').getAttribute('aria-disabled') === 'false'")
        self.assertEqual(evaluate('document.activeElement.id'), 'add-project')
        self.assertGreaterEqual(evaluate("document.querySelector('.terminal').getBoundingClientRect().width"), 640)
        browser('click', '[data-id=folder-9]')
        browser('wait', '--fn', "document.querySelector('[data-id=folder-9]').getAttribute('aria-current') === 'true'")
        browser('click', '[data-id=project-7]')
        browser('wait', '--fn', "document.querySelector('[data-id=project-7]').getAttribute('aria-expanded') === 'false'")
        self.assertEqual(evaluate('window.sidebarFixture.selected'), 'folder-9')
        browser('press', 'Space')
        browser('wait', '--fn', "document.querySelector('[data-id=project-7]').getAttribute('aria-expanded') === 'true'")
        for width in [1280, 375]:
            browser('set', 'viewport', str(width), '900')
            if width == 375:
                browser('click', '#projects-toggle')
                self.assertTrue(evaluate("document.querySelector('#projects').matches(':modal')"))
                for _ in range(8):
                    browser('press', 'Tab')
                    self.assertTrue(evaluate("document.querySelector('#projects').contains(document.activeElement)"))
                self.assertEqual(evaluate("document.querySelector('#projects').getBoundingClientRect().bottom"), evaluate("document.querySelector('#status-bar').getBoundingClientRect().top"))
            for theme in ['light', 'dark']:
                evaluate(f"document.documentElement.dataset.theme = '{theme}'")
                self.assertTrue(evaluate('document.documentElement.scrollWidth <= innerWidth'))
                browser('screenshot', str(SHOTS / f'sidebar-fixture-{width}-{theme}.png'))
                if reference:
                    query = urlencode(dict(id='sidebar/orquesta-reference:reference', theme=theme))
                    ref = lambda *args: subprocess.check_output(['npx', '--yes', 'agent-browser@0.39.4', '--session', 'sidebar-reference', *args], text=True)
                    ref('open', f'http://127.0.0.1:5179/fixtures.html?{query}')
                    ref('set', 'viewport', str(width), '900')
                    ref('wait', '--fn', "document.documentElement.dataset.fixtureState === 'ready'")
                    ref('screenshot', str(SHOTS / f'sidebar-host-orca-{width}-{theme}.png'))
            if width == 375:
                browser('click', '[data-id=folder-9]')
                browser('wait', '--fn', "document.querySelector('#projects').hidden && document.activeElement.id === 'projects-toggle'")
                browser('click', '#projects-toggle')
                browser('press', 'Escape')
                self.assertEqual(evaluate('document.activeElement.id'), 'projects-toggle')
        self.assertTrue(evaluate("window.sidebarRequests.every(r => !['check', 'read', 'ack'].includes(r.command))"))
        self.assertFalse(ipc(dict(command='inbox', recipient='session:s'))['result'][0]['read'])
        after = self.draft_state()
        for key in ['sameNode', 'value', 'start', 'end', 'direction', 'sent']:
            self.assertEqual(after[key], before[key])
