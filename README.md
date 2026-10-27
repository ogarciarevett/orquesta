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

### Internal desktop preview

The native workspace packages the static terminal draft, durable inbox, and status bar in Tauri v2. It invokes the Rust runtime directly, stores its fixed session mailbox under the application data directory, and keeps unavailable usage and inventories explicit. The Python host remains development only. Draft submission is disabled in the native preview because a PTY is not connected.

The Desktop workflow produces an Apple Silicon app ZIP with an ad hoc signature, SHA256, source revision, toolchain, dependency lock, and WebView smoke report. This internal artifact is not notarized. A draft without its tracked generated lock is a dependency-sizing candidate, not a reproducible release. Script CSP stays enabled; style attributes are allowed only for the existing usage meter widths. Production assets are explicitly allowlisted; reference fixtures, development hosts, and smoke scripts are excluded. No shell, filesystem, HTTP, updater, or remote-origin permissions are granted to the webview.

Selected-folder catalog (#110, first core slice): trusted native code can register
an existing local folder through `Mailbox::sidebar_add_project`; this API must
never accept a webview-supplied path. Canonical roots remain in mailbox storage;
`sidebar_snapshot` projects opaque project/folder IDs and labels only. Folders
need not be Git repositories. Selection and host/project collapse preferences
use catalog IDs and revision checks and persist in the same atomic transaction
as existing mail and status preferences. Cancellation makes no storage change.
The desktop sidebar opens a native existing-folder picker through Add project;
selection and collapse are stored using the catalog revision. At 1280px the inbox
opens as an overlay so the 280px sidebar retains a 640px main area. At <=920px,
Projects opens a sheet; selecting a folder or Escape closes it and returns focus.
Git discovery, remote hosts, filtering, resizing, multi-selection, and live agents
remain outside this slice; a folder row does not claim Git or agent status.

### Inactive automation drafts (#39, core foundation)

`Mailbox::automation_snapshot` and `automation_save(expected_revision, id, draft)`
persist interval drafts against a registered project/workspace pair. `None` creates
a native `automation-N` ID; `Some(id)` replaces a known draft. Names are trimmed
and limited to 120 UTF-8 bytes; prompts retain their exact bytes, must contain
non-whitespace text, and are limited to 32 KiB. Intervals are 60–31,536,000 seconds
with UTC anchors from 0 through 253,402,300,799; at most 128 drafts are stored.
These are inactive definitions, with no scheduling, timezone validation, execution,
history, provider configuration, UI, native command, or Runtime forwarding.

Old mailbox files acquire an empty schema-1 catalog while retaining mail and
preferences. Saves check revision and target membership under the existing lock;
stale retries conflict instead of duplicating creates. Reload the snapshot before
retrying any save error: failures after rename can mean the write committed.
Unknown automation fields/schema and invalid drafts fail without resetting state.
Do not write this storage with older binaries: they may discard automation data.
Prompts are private plaintext in mailbox storage; snapshots contain no root fields.
