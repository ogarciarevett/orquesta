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
fn shell_runs_in_its_folder_with_send_read_and_submit_confirmation() -> Result<()> {
    let root = folder("pty-folder")?;
    let mut terminal = Terminal::spawn(&root, "/bin/sh", &[])?;
    // The computed value appears only in command output, never in the echo.
    assert!(terminal.send("echo $((40+2))z", true, Some(WAIT))?);
    assert!(shows(&terminal, 0, "42z"));
    let cursor = terminal.read(0).next;
    assert!(terminal.send("pwd", true, Some(WAIT))?);
    assert!(shows(&terminal, cursor, &root.display().to_string()));
    let chunk = terminal.read(cursor);
    assert_eq!(
        (chunk.cursor, chunk.truncated, chunk.closed),
        (cursor, false, false)
    );
    assert!(terminal.send("two\nlines", true, None).is_err());
    terminal.resize(40, 120)?;
    assert!(terminal.resize(0, 120).is_err());
    terminal.send("exit 3", true, None)?;
    terminal.wait_for(0, WAIT, |_| false);
    assert!(terminal.read(0).closed);
    assert_eq!(terminal.close()?, 3);
    Ok(())
}

#[test]
fn interrupt_and_close_stop_only_the_terminal_process() -> Result<()> {
    let root = folder("pty-close")?;
    let mut shell = Terminal::spawn(&root, "/bin/sh", &[])?;
    shell.send("sleep 30", true, Some(WAIT))?;
    // Give sleep time to become the foreground process before Ctrl-C.
    shell.wait_for(0, Duration::from_millis(500), |_| false);
    shell.interrupt()?;
    let cursor = shell.read(0).next;
    shell.send("echo $((1+1))done", true, None)?;
    assert!(shows(&shell, cursor, "2done"));
    let mut sleeper = Terminal::spawn(&root, "sleep", &["30"])?;
    assert_eq!(sleeper.exit_code()?, None);
    assert_ne!(sleeper.close()?, 0);
    assert_eq!(shell.exit_code()?, None);
    shell.close()?;
    assert!(Terminal::spawn(&root.join("missing"), "/bin/sh", &[]).is_err());
    Ok(())
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
            .send_automation(&mut terminal, "echo automation", true, None)
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
    assert!(composer.send_automation(&mut terminal, "echo $((2*4))ok", true, Some(WAIT))?);
    assert!(shows(&terminal, cursor, "8ok"));
    terminal.close()?;
    Ok(())
}
