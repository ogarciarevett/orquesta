# Copyright (c) 2026 Lovecast Inc.
# SPDX-License-Identifier: MIT
"""Loopback development host for the Rust mailbox UI."""
import http.server
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
PORT = int(os.environ.get("ORQUESTA_UI_PORT", "8765"))
os.environ.setdefault("ORQUESTA_MAILBOX_DIR", str(ROOT / "target" / "inbox-ui"))
subprocess.run(["cargo", "build", "--bin", "orquesta", "--example", "inbox_seed"], cwd=ROOT, check=True)
subprocess.run([str(ROOT / "target/debug/examples/inbox_seed")], check=True)

FIXTURES = os.environ.get("ORQUESTA_USAGE_FIXTURES")


def fixtures():
    try:
        return json.loads(Path(FIXTURES).read_text()) if FIXTURES else {}
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def status_call(action):
    request = {"version": 1, "operation": {"command": "status-bar", "action": action}}
    result = subprocess.run([str(ROOT / "target/debug/orquesta"), "ipc"], input=json.dumps(request).encode(), capture_output=True)
    if result.returncode:
        raise ValueError("Usage refresh failed")
    return json.loads(result.stdout)


def refresh(data):
    source = fixtures().get(data["id"])
    if source is None:
        return {"version": 1, "result": {"unavailable": True}, "events": []}
    claim = status_call(dict(action="begin", id=data["id"], request_id=data["request_id"]))
    if claim["result"]["started"]:
        with open(FIXTURES + ".calls", "a") as calls:
            calls.write(data["id"] + "\n")
        time.sleep(min(source.get("delay", 0), 15))
        result = {key: source[key] for key in ("phase", "windows", "plan") if key in source}
        return status_call(dict(action="finish", id=data["id"], token=claim["result"]["token"], now=int(time.time()), **result))
    return claim


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT / "ui"), **kwargs)

    def do_POST(self):
        origin = f"http://127.0.0.1:{PORT}"
        if self.path not in ("/ipc", "/refresh") or self.headers.get("Origin", origin) != origin:
            self.send_error(403)
            return
        size = int(self.headers.get("Content-Length", "0"))
        if self.headers.get_content_type() != "application/json" or not 0 < size <= 65536:
            self.send_error(400)
            return
        payload = self.rfile.read(size)
        if self.path == "/refresh":
            try:
                reply, code = refresh(json.loads(payload)), 0
            except (KeyError, ValueError):
                reply, code = {"error": {"message": "Usage refresh failed"}}, 1
        else:
            result = subprocess.run([str(ROOT / "target/debug/orquesta"), "ipc"], input=payload, capture_output=True)
            code = result.returncode
            reply = json.loads(result.stderr if code else result.stdout)
        if not code and isinstance(reply["result"], dict) and "snapshot" in reply["result"]:
            reply["result"]["resources"] = {"load": os.getloadavg()[0] if hasattr(os, "getloadavg") else None}
            reply["result"]["refresh_providers"] = list(fixtures())
        self.send_response(400 if code else 200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(json.dumps(reply).encode())


http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
