# Copyright (c) 2026 Lovecast Inc.
# SPDX-License-Identifier: MIT
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
URL = "http://127.0.0.1:8765"
SHOTS = ROOT / "artifacts" / "inbox-ui"


def browser(*args):
    command = ["npx", "--yes", "agent-browser@0.39.4", "--session", f"inbox-test-{os.getpid()}"]
    return subprocess.check_output(command + list(args), text=True)


def evaluate(script):
    return json.loads(browser("eval", script, "--json"))["data"]["result"]


def ipc(operation):
    request = urllib.request.Request(URL + "/ipc", json.dumps({"version": 1, "operation": operation}).encode(),
                                     {"Content-Type": "application/json"})
    with urllib.request.urlopen(request) as response:
        return json.load(response)


class InboxUI(unittest.TestCase):
    def test_durable_inbox_never_changes_human_input(self):
        with tempfile.TemporaryDirectory() as directory:
            env = dict(os.environ, ORQUESTA_MAILBOX_DIR=directory)
            server = subprocess.Popen(["python3", "ui/serve.py"], cwd=ROOT, env=env, stdout=subprocess.DEVNULL)
            try:
                for attempt in range(600):
                    try:
                        urllib.request.urlopen(URL).close()
                        break
                    except OSError:
                        time.sleep(0.1)
                else:
                    self.fail("UI host failed to start")
                SHOTS.mkdir(parents=True, exist_ok=True)
                browser("open", URL)
                browser("set", "viewport", "1280", "900")
                browser("fill", "#draft", "first line\nsecond λ line\nlast line")
                evaluate("document.querySelector('#draft').setSelectionRange(4, 15, 'backward')")
                state = "(() => { const d = document.querySelector('#draft'); return [d.value, d.selectionStart, d.selectionEnd, d.selectionDirection, document.querySelector('#submissions').textContent]; })()"
                before = evaluate(state)
                self.assertEqual(before[4], "0")
                for number in range(3):
                    envelope = dict(request_id=f"ui-{number}", to="session:s", **{"from": "run:r"},
                                    kind="status", subject=f"Update {number}", body="<script>bad()</script>", thread=None)
                    ipc(dict(command="send", envelope=envelope))
                browser("wait", "--fn", "document.querySelectorAll('.message').length === 3")
                self.assertEqual(evaluate(state), before)
                self.assertEqual(evaluate("document.activeElement.id"), "draft")
                self.assertEqual(evaluate("document.querySelector('#badge').textContent.trim()"), "3")
                self.assertEqual(evaluate("[...document.querySelectorAll('.message')].map(m => m.dataset.id)"), ["1", "2", "3"])
                self.assertEqual(evaluate("document.querySelector('#inbox-toggle').getAttribute('aria-label')"), "Inbox, 3 unread")
                self.assertTrue(evaluate("document.querySelectorAll('.message time[datetime]').length === 3 && [...document.querySelectorAll('time[datetime]')].every(t => t.title === t.dateTime)"))
                for width in [1280, 375]:
                    browser("set", "viewport", str(width), "900")
                    for theme in ["light", "dark"]:
                        if evaluate("document.documentElement.dataset.theme") != theme:
                            browser("click", "#theme")
                        self.assertTrue(evaluate("document.documentElement.scrollWidth <= innerWidth"))
                        browser("screenshot", str(SHOTS / f"{width}-{theme}.png"))
                        if width == 375:
                            browser("click", "#inbox-toggle")
                            self.assertTrue(evaluate("document.querySelector('#inbox').matches(':modal') && document.querySelector('#inbox').getBoundingClientRect().height >= innerHeight - 2"))
                            browser("screenshot", str(SHOTS / f"375-sheet-open-{theme}.png"))
                            browser("press", "Escape")
                            self.assertEqual(evaluate("document.activeElement.id"), "inbox-toggle")
                browser("click", "#inbox-toggle")
                browser("click", "#inbox-settings")
                browser("uncheck", "#safety")
                browser("press", "Escape")
                self.assertTrue(evaluate("document.querySelector('#inbox').open && !document.querySelector('#settingsdialog').open"))
                browser("click", "#inbox-close")
                browser("set", "viewport", "1280", "900")
                browser("screenshot", str(SHOTS / "unread-row.png"))
                browser("focus", "#draft")
                browser("press", "Escape")
                self.assertTrue(evaluate("document.querySelector('#inbox').open"))
                browser("click", "#read-batch")
                browser("wait", "--fn", "document.querySelector('#badge').hidden")
                self.assertEqual(evaluate("document.querySelector('#inbox-toggle').getAttribute('aria-label')"), "Inbox")
                browser("click", "[data-ack='1']")
                browser("wait", "--text", "Acknowledged")
                self.assertEqual(evaluate("document.querySelectorAll('[data-ack=\"1\"]').length"), 0)
                browser("click", "#connection")
                browser("screenshot", str(SHOTS / "disconnected.png"))
                self.assertTrue(evaluate("document.querySelector('#inbox #status').textContent.includes('Disconnected') && document.querySelector('#last-good').textContent.length > 0"))
                browser("click", "#retry")
                self.assertEqual(evaluate(state), before)
                self.assertEqual(evaluate("document.querySelectorAll('.message').length"), 3)
                self.assertTrue(evaluate("document.querySelector('.message').textContent.includes('<script>bad()</script>')"))
                saved_theme = evaluate("document.documentElement.dataset.theme")
                browser("reload")
                self.assertEqual(evaluate("document.documentElement.dataset.theme"), saved_theme)
                self.assertFalse(evaluate("document.querySelector('#safety').checked"))
                browser("press", "Tab")
                self.assertTrue(evaluate("document.activeElement !== document.body"))
                rows = ipc(dict(command="inbox", recipient="session:s"))["result"]
                self.assertEqual(len(rows), 3)
                self.assertTrue(rows[0]["acknowledged"])
                self.assertFalse(rows[1]["acknowledged"])
            finally:
                try:
                    browser("close")
                finally:
                    server.terminate()
                    server.wait()


if __name__ == "__main__":
    unittest.main()
