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
