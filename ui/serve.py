# Copyright (c) 2026 Lovecast Inc.
# SPDX-License-Identifier: MIT
"""Loopback development host for the Rust mailbox UI."""
import http.server
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent
PORT = int(os.environ.get("ORQUESTA_UI_PORT", "8765"))
os.environ.setdefault("ORQUESTA_MAILBOX_DIR", str(ROOT / "target" / "inbox-ui"))
subprocess.run(["cargo", "build", "--bin", "orquesta", "--example", "inbox_seed"], cwd=ROOT, check=True)
subprocess.run([str(ROOT / "target/debug/examples/inbox_seed")], check=True)


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT / "ui"), **kwargs)

    def do_POST(self):
        origin = f"http://127.0.0.1:{PORT}"
        if self.path != "/ipc" or self.headers.get("Origin", origin) != origin:
            self.send_error(403)
            return
        size = int(self.headers.get("Content-Length", "0"))
        if self.headers.get_content_type() != "application/json" or not 0 < size <= 65536:
            self.send_error(400)
            return
        result = subprocess.run([str(ROOT / "target/debug/orquesta"), "ipc"],
                                input=self.rfile.read(size), capture_output=True)
        self.send_response(200 if result.returncode == 0 else 400)
        self.send_header("Content-Type", "application/json")
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(result.stdout if result.returncode == 0 else result.stderr)


http.server.HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
