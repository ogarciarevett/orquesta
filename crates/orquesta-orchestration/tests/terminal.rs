// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
#![cfg(unix)]
use orquesta_orchestration::{Result, terminal::Terminal};
use std::{path::PathBuf, process::Stdio, time::Duration};

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
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn interrupt_and_close_stop_only_the_terminal_process() -> Result<()> {
    let root = folder("pty-close")?;
    let mut shell = Terminal::spawn(&root, "/bin/sh", &[])?;
    assert!(shell.send("echo $((3+3))go; sleep 30", true, Some(WAIT))?);
    assert!(shows(&shell, 0, "6go"));
    // Give sleep time to become the foreground process before Ctrl-C.
    shell.wait_for(0, Duration::from_millis(300), |_| false);
    shell.interrupt()?;
    let cursor = shell.read(0).next;
    assert!(shell.send("echo $((1+1))done", true, Some(WAIT))?);
    assert!(shows(&shell, cursor, "2done"));
    let mut sleeper = Terminal::spawn(&root, "sleep", &["30"])?;
    assert_eq!(sleeper.exit_code()?, None);
    assert_ne!(sleeper.close()?, 0);
    assert_eq!(shell.exit_code()?, None);
    shell.close()?;
    assert!(Terminal::spawn(&root.join("missing"), "/bin/sh", &[]).is_err());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn pending_input_is_tracked_and_drop_stops_the_process() -> Result<()> {
    let root = folder("pty-pending")?;
    let mut terminal = Terminal::spawn(&root, "/bin/sh", &[])?;
    terminal.send("echo $((5+5))", false, None)?;
    assert!(terminal.has_pending_input());
    assert!(terminal.send("t", true, Some(WAIT))?);
    assert!(shows(&terminal, 0, "10t") && !terminal.has_pending_input());
    terminal.send("exit", true, None)?;
    terminal.wait_for(0, WAIT, |_| false);
    assert!(terminal.read(0).closed && terminal.send("echo late", true, None).is_err());
    let sleeper = Terminal::spawn(&root, "/bin/sh", &["-c", "echo pid=$$; exec sleep 30"])?;
    assert!(sleeper.wait_for(0, WAIT, |b| b.ends_with(b"\n")));
    let pid = sleeper
        .read(0)
        .text
        .trim()
        .trim_start_matches("pid=")
        .to_owned();
    drop(sleeper);
    let alive = std::process::Command::new("kill")
        .args(["-0", &pid])
        .stderr(Stdio::null())
        .status()?;
    assert!(
        !alive.success(),
        "dropped terminal left process {pid} running"
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}
