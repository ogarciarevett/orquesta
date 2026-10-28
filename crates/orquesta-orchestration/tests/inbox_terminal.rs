// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
#![cfg(unix)]
use orquesta_orchestration::{
    Envelope, Mailbox, Result, composer::SessionComposer, runtime::Runtime, terminal::Terminal,
};
use std::{path::PathBuf, time::Duration};

const WAIT: Duration = Duration::from_secs(10);

fn folder(name: &str) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("orquesta-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&path)?;
    Ok(path.canonicalize()?)
}

fn shows(terminal: &Terminal, cursor: usize, needle: &str) -> bool {
    terminal.wait_for(cursor, WAIT, |bytes| {
        String::from_utf8_lossy(bytes).contains(needle)
    })
}

fn prompt(bytes: &[u8]) -> bool {
    bytes.ends_with(b"$ ") || bytes.ends_with(b"# ")
}

#[test]
fn inbox_mail_never_types_over_the_human_draft() -> Result<()> {
    let root = folder("pty-inbox")?;
    let runtime = Runtime(Mailbox::open(root.join("mailbox"))?);
    for address in ["run:r", "session:s"] {
        runtime.0.register(address)?;
    }
    let mut terminal = Terminal::spawn(&root, "/bin/sh", &[])?;
    let mut composer = SessionComposer::new("session:s".into());
    composer.reconnect(&runtime)?;
    composer.edit("echo $((6*7))x\necho $((7*7))y");
    composer.select(3, Some((2, 5)))?;
    let draft = (
        composer.draft().text.clone(),
        composer.draft().cursor,
        composer.draft().selection,
    );
    for n in 0..3 {
        runtime.0.send(Envelope {
            request_id: format!("mail-{n}"),
            to: "session:s".into(),
            from: "run:r".into(),
            kind: "status".into(),
            subject: format!("update {n}"),
            body: "type this into the terminal".into(),
            thread: None,
            metadata: Default::default(),
        })?;
    }
    composer.reconnect(&runtime)?;
    assert!(terminal.wait_for(0, WAIT, prompt));
    let before = terminal.read(0).next;
    assert!(
        composer
            .send_automation(&mut terminal, "echo automation", None)
            .is_err()
    );
    assert!(!terminal.wait_for(before, Duration::from_millis(300), |b| !b.is_empty()));
    let after = (
        composer.draft().text.clone(),
        composer.draft().cursor,
        composer.draft().selection,
    );
    assert_eq!(after, draft);
    let subjects: Vec<_> = composer.inbox.iter().map(|m| &m.envelope.subject).collect();
    assert_eq!(subjects, ["update 0", "update 1", "update 2"]);
    assert!(composer.submit_to(&mut terminal, Some(WAIT))?);
    assert!(shows(&terminal, before, "42x") && shows(&terminal, before, "49y"));
    assert_eq!(
        (composer.submissions(), composer.draft().text.as_str()),
        (1, "")
    );
    let cursor = terminal.read(0).next;
    assert!(composer.send_automation(&mut terminal, "echo $((2*4))ok", Some(WAIT))?);
    assert!(shows(&terminal, cursor, "8ok"));
    terminal.close()?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn automation_waits_for_pending_input_and_closed_terminals_keep_the_draft() -> Result<()> {
    let root = folder("pty-pending-draft")?;
    let mut terminal = Terminal::spawn(&root, "/bin/sh", &[])?;
    let mut composer = SessionComposer::new("session:s".into());
    terminal.send("echo $((5+5))", false, None)?;
    assert!(
        composer
            .send_automation(&mut terminal, "echo automation", None)
            .is_err()
    );
    assert!(terminal.send("t", true, Some(WAIT))?);
    assert!(shows(&terminal, 0, "10t") && !terminal.read(0).text.contains("automation"));
    terminal.send("exit", true, None)?;
    terminal.wait_for(0, WAIT, |_| false);
    composer.edit("echo kept");
    assert!(composer.submit_to(&mut terminal, None).is_err());
    assert_eq!(
        (composer.draft().text.as_str(), composer.submissions()),
        ("echo kept", 0)
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}
