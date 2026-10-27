# Copyright (c) 2026 Lovecast Inc.
# Copyright (c) 2026 Omar Garcia
# SPDX-License-Identifier: MIT
"""Focused browser regressions using the real development host and runtime."""
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
import urllib.request
from test_ui import ROOT, URL, SHOTS, browser, evaluate, ipc
from test_status_bar import verify_checkbox_poll_focus


class FocusUI(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        os.environ['ORQUESTA_MAILBOX_DIR'] = self.directory.name
        os.environ['ORQUESTA_USAGE_FIXTURES'] = str(Path(self.directory.name) / 'usage.json')
        self.server = subprocess.Popen(['python3', 'ui/serve.py'], cwd=ROOT, stdout=subprocess.DEVNULL)
        self.addCleanup(self.close_host)
        for attempt in range(600):
            try:
                urllib.request.urlopen(URL).close()
                break
            except OSError:
                time.sleep(.1)
        else:
            self.fail('Focus UI host failed to start')
        SHOTS.mkdir(parents=True, exist_ok=True)
        browser('open', URL)
        browser('set', 'viewport', '1280', '900')
        browser('wait', '--fn', "document.querySelector('#status').textContent === 'Connected' && !!document.querySelector('#sb-menu input[data-key=claude]')")

    def close_host(self):
        try:
            browser('close')
        finally:
            self.server.terminate()
            self.server.wait()
            self.directory.cleanup()

    def draft_state(self):
        return evaluate("""(() => {
          const draft = document.querySelector('#draft');
          return {sameNode: draft === window.focusDraft, value: draft.value,
            start: draft.selectionStart, end: draft.selectionEnd,
            direction: draft.selectionDirection, focus: document.activeElement.id,
            sent: document.querySelector('#submissions').value, focusLosses: window.focusLosses};
        })()""")

    def prepare_draft(self):
        browser('fill', '#draft', 'first line\nsecond λ line\nlast line')
        evaluate("window.focusDraft = document.querySelector('#draft'); window.focusDraft.setSelectionRange(4, 22, 'backward')")
        evaluate("window.focusLosses = 0; window.focusDraft.addEventListener('blur', () => window.focusLosses++)")
        before = self.draft_state()
        self.assertEqual(before['focus'], 'draft')
        self.assertEqual(before['sent'], '0')
        self.assertEqual((before['start'], before['end'], before['direction']), (4, 22, 'backward'))
        return before

    def test_composer_survives_polls_and_both_resize_directions(self):
        before = self.prepare_draft()
        for count, width in enumerate([699, 701, 375, 1280], 1):
            with self.subTest(width=width):
                browser('focus', '#draft')
                self.assertEqual(self.draft_state(), before)
                browser('set', 'viewport', str(width), '900')
                expanded = 'true' if width > 700 else 'false'
                browser('wait', '--fn', f"document.querySelector('#inbox-toggle').getAttribute('aria-expanded') === '{expanded}'")
                envelope = dict(request_id=f'focus-{count}', to='session:s', **{'from': 'run:r'},
                                kind='status', subject=f'Resize {width}', body='Separate inbox notice', thread=None)
                ipc(dict(command='send', envelope=envelope))
                browser('wait', '--fn', f"document.querySelectorAll('.message').length === {count}")
                ipc(dict(command='status-bar', action=dict(action='inventory', kind='terminals', id=f'resize-{count}', present=True)))
                browser('wait', '--fn', f"document.querySelector('#sb-terminals').ariaLabel === '{count} terminals'")
                browser('screenshot', str(SHOTS / f'focus-resize-{width}.png'))
                print(f'Resize {width}: mailbox and status markers rendered', flush=True)
                self.assertEqual(self.draft_state(), before)
        browser('set', 'viewport', '375', '900')
        browser('wait', '--fn', "!document.querySelector('#inbox').open")
        for dismissal in ['Escape', 'Close']:
            browser('click', '#inbox-toggle')
            self.assertTrue(evaluate("document.querySelector('#inbox').matches(':modal')"))
            self.assertEqual(evaluate("document.querySelector('#inbox-toggle').getAttribute('aria-expanded')"), 'true')
            if dismissal == 'Escape':
                browser('press', 'Escape')
            else:
                browser('click', '#inbox-close')
            browser('wait', '--fn', "!document.querySelector('#inbox').open && document.activeElement.id === 'inbox-toggle'")
            expected = dict(before, focus='inbox-toggle', focusLosses=1)
            self.assertEqual(self.draft_state(), expected)
            self.assertEqual(evaluate("document.querySelector('#inbox-toggle').getAttribute('aria-expanded')"), 'false')

    def test_hidden_inbox_controls_return_focus_to_visible_trigger(self):
        ipc(dict(command='send', envelope=dict(request_id='hidden-target', to='session:s',
            **{'from': 'run:r'}, kind='status', subject='Focusable notice', body='Inbox target', thread=None)))
        browser('wait', '--fn', "document.querySelectorAll('.message').length === 1")
        before = self.prepare_draft()
        for count, selector in enumerate(['#inbox-close', '#messages .message'], 1):
            with self.subTest(selector=selector):
                browser('set', 'viewport', '1280', '900')
                browser('wait', '--fn', "document.querySelector('#inbox').open")
                browser('focus', selector)
                self.assertTrue(evaluate(f"document.activeElement === document.querySelector('{selector}')"))
                browser('set', 'viewport', '375', '900')
                browser('wait', '--fn', "!document.querySelector('#inbox').open && document.activeElement.id === 'inbox-toggle'")
                ipc(dict(command='status-bar', action=dict(action='inventory', kind='terminals', id=f'hidden-{count}', present=True)))
                browser('wait', '--fn', f"document.querySelector('#sb-terminals').ariaLabel === '{count} terminals'")
                self.assertEqual(evaluate("document.querySelector('#inbox-toggle').getAttribute('aria-expanded')"), 'false')
                self.assertEqual(self.draft_state(), dict(before, focus='inbox-toggle', focusLosses=1))
                browser('screenshot', str(SHOTS / f'focus-hidden-{count}.png'))
                print(f'{selector}: hidden panel returned focus after successful poll', flush=True)

    def test_explicit_dismissal_returns_trigger_after_native_focus_restore(self):
        browser('set', 'viewport', '375', '900')
        browser('wait', '--fn', "!document.querySelector('#inbox').open")
        before = self.prepare_draft()
        evaluate("document.querySelector('#inbox-toggle').click()")
        self.assertTrue(evaluate("document.querySelector('#inbox').matches(':modal')"))
        browser('press', 'Escape')
        browser('wait', '--fn', "!document.querySelector('#inbox').open && document.activeElement.id === 'inbox-toggle'")
        after = self.draft_state()
        for key in ['sameNode', 'value', 'start', 'end', 'direction', 'sent']:
            self.assertEqual(after[key], before[key])
        self.assertGreater(after['focusLosses'], 0)

    def test_checkbox_space_persists_across_successful_menu_and_settings_polls(self):
        for surface in ['sb-menu', 'sb-settings']:
            with self.subTest(surface=surface):
                browser('focus', '#sb-items')
                browser('press', 'ContextMenu')
                if surface == 'sb-settings':
                    browser('click', '#sb-settings-open')
                selector = f'#{surface} input[data-key=claude]'
                checked = verify_checkbox_poll_focus(self, browser, evaluate, ipc, selector)
                browser('screenshot', str(SHOTS / f'focus-{surface}.png'))
                browser('press', 'Escape')
                browser('reload')
                browser('wait', '--fn', f"!!document.querySelector('{selector}') && document.querySelector('{selector}').checked === {str(checked).lower()}")
                self.assertEqual(ipc(dict(command='status-bar', action=dict(action='snapshot')))['result']['snapshot']['visibility']['claude'], checked)


if __name__ == '__main__':
    unittest.main(verbosity=2)
