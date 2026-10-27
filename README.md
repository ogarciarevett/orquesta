# orquesta
Open source agent orchestration desktop app in Rust and Tauri v2

The orchestration crate provides durable addressed inboxes. Register trusted
recipients before sending. Reads, delivery replay, and acknowledgements operate
on mailbox storage, independently of the human terminal draft. The terminal
buffer exposes no inbox delivery or submission method.

Run `cargo test` and `cargo clippy` to validate the workspace. Cargo.lock is
currently omitted for this library workspace.

Orca reference: https://github.com/stablyai/orca, MIT License.
Copyright (c) 2026 Lovecast Inc. See THIRD_PARTY_NOTICES.md for the full notice.

The local CLI reads ORQUESTA_MAILBOX_DIR, ORQUESTA_RUN_ID, and ORQUESTA_FROM.
Trusted runtime code must register recipients first. Run `cargo run --bin
orquesta -- schema` for the six-command registry. Remote capabilities and
Tauri IPC integration remain pending; unsupported connections fail explicitly.

`orquesta ipc` reads one versioned JSON Request from stdin and returns a Response
with separate MailEvent notifications. Version 1 dispatches mailbox commands;
unsupported versions fail. SessionComposer keeps draft editing and explicit
submission separate from inbox notifications, acknowledgement, and reconnect.

Run `python3 ui/serve.py` and open http://127.0.0.1:8765 for the responsive
inbox UI. This loopback development bridge calls the Rust IPC executable and
registers run:r and session:s in ORQUESTA_MAILBOX_DIR (default target/inbox-ui).
Mail stays in Rust storage; notification and theme preferences stay in this
browser. Draft protection is always on. Submission is a local counter only.
Run `python3 ui/test_ui.py` for browser verification (Node.js is required).
