# Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
"""Projection fixtures exercise UI only; they do not prove native picker operation."""
import unittest
import test_focus
from test_ui import browser, evaluate, SHOTS


class SidebarUI(unittest.TestCase):
    setUp = test_focus.FocusUI.setUp
    close_host = test_focus.FocusUI.close_host
    draft_state = test_focus.FocusUI.draft_state
    prepare_draft = test_focus.FocusUI.prepare_draft
    def test_projection_selection_collapse_and_draft(self):
        before = self.prepare_draft()
        evaluate("""window.sidebarFixture = {revision: 1, selected: null, collapsed: [], projects: [
          {id: 'project-7', workspace_id: 'folder-9', label: '日本語 project', host_id: 'local', kind: 'folder'}]};
          window.sidebarRequests = [];
          window.orquestaNative = async (command, fields) => {
            window.sidebarRequests.push({command, fields});
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
            for theme in ['light', 'dark']:
                evaluate(f"document.documentElement.dataset.theme = '{theme}'")
                self.assertTrue(evaluate('document.documentElement.scrollWidth <= innerWidth'))
                browser('screenshot', str(SHOTS / f'sidebar-fixture-{width}-{theme}.png'))
            if width == 375:
                browser('click', '[data-id=folder-9]')
                browser('wait', '--fn', "document.querySelector('#projects').hidden && document.activeElement.id === 'projects-toggle'")
                browser('click', '#projects-toggle')
                browser('press', 'Escape')
                self.assertEqual(evaluate('document.activeElement.id'), 'projects-toggle')
        self.assertTrue(evaluate("window.sidebarRequests.every(r => ['sidebar', 'sidebar-preference'].includes(r.command))"))
        after = self.draft_state()
        for key in ['sameNode', 'value', 'start', 'end', 'direction', 'sent']:
            self.assertEqual(after[key], before[key])
