// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
use crate::{Result, invalid};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    path::Path,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

/// Scrollback kept in memory (1 to 2 MiB, trimmed in bulk); older bytes are
/// dropped but cursors stay absolute.
const SCROLLBACK: usize = 1 << 20;

#[derive(Default)]
struct Output {
    base: usize,
    bytes: Vec<u8>,
    closed: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Chunk {
    pub text: String,
    pub cursor: usize,
    pub next: usize,
    pub truncated: bool,
    pub closed: bool,
}

/// One real pseudo terminal running a process inside a workspace folder.
pub struct Terminal {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    output: Arc<(Mutex<Output>, Condvar)>,
    pending: bool,
}

/// Length of `bytes` without a trailing partial UTF-8 character, so a character
/// split across two PTY reads is returned whole by the next read.
pub(crate) fn complete(bytes: &[u8]) -> usize {
    for back in 1..=bytes.len().min(3) {
        let byte = bytes[bytes.len() - back];
        if byte & 0xC0 != 0x80 {
            let width = match byte {
                0xC0..=0xDF => 2,
                0xE0..=0xEF => 3,
                0xF0..=0xF7 => 4,
                _ => 1,
            };
            return if width > back {
                bytes.len() - back
            } else {
                bytes.len()
            };
        }
    }
    bytes.len()
}

impl Terminal {
    /// Starts `program` with `args` on a new PTY whose working directory is `cwd`.
    pub fn spawn(cwd: &Path, program: &str, args: &[&str]) -> Result<Self> {
        if !cwd.is_dir() {
            return Err(invalid("terminal folder must be an existing directory"));
        }
        let pair = native_pty_system().openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut command = CommandBuilder::new(program);
        command.args(args);
        command.cwd(cwd);
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let output = Arc::new((Mutex::new(Output::default()), Condvar::new()));
        let sink = output.clone();
        std::thread::spawn(move || {
            let mut buffer = [0; 8192];
            loop {
                let count = match reader.read(&mut buffer) {
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    result => result.unwrap_or(0),
                };
                let (lock, ready) = &*sink;
                let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
                if count == 0 {
                    state.closed = true;
                    ready.notify_all();
                    return;
                }
                state.bytes.extend_from_slice(&buffer[..count]);
                if state.bytes.len() > 2 * SCROLLBACK {
                    let excess = state.bytes.len() - SCROLLBACK;
                    state.bytes.drain(..excess);
                    state.base += excess;
                }
                ready.notify_all();
            }
        });
        Ok(Self {
            master: pair.master,
            writer,
            child,
            output,
            pending: false,
        })
    }

    fn end(&self) -> usize {
        let state = self.output.0.lock().unwrap_or_else(|e| e.into_inner());
        state.base + state.bytes.len()
    }

    /// Returns output from `cursor`; `next` is the cursor for the following read.
    pub fn read(&self, cursor: usize) -> Chunk {
        let state = self.output.0.lock().unwrap_or_else(|e| e.into_inner());
        let start = cursor.clamp(state.base, state.base + state.bytes.len());
        let mut bytes = &state.bytes[start - state.base..];
        if !state.closed {
            bytes = &bytes[..complete(bytes)];
        }
        Chunk {
            text: String::from_utf8_lossy(bytes).into_owned(),
            cursor: start,
            next: start + bytes.len(),
            truncated: start > cursor,
            closed: state.closed,
        }
    }

    /// Waits until output after `cursor` satisfies `done` or the terminal closes.
    pub fn wait_for(&self, cursor: usize, timeout: Duration, done: impl Fn(&[u8]) -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        let (lock, ready) = &*self.output;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            let start = cursor.saturating_sub(state.base).min(state.bytes.len());
            if done(&state.bytes[start..]) {
                return true;
            }
            let now = Instant::now();
            if state.closed || now >= deadline {
                return false;
            }
            state = ready
                .wait_timeout(state, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    /// Writes one line of text. With `enter`, presses Enter and, when `wait` is
    /// set, confirms submission by waiting for a line break produced after Enter.
    /// shortcut: the line break is the cooked-mode echo of Enter, so it proves the
    /// line was taken, not that a busy process read it, and late output from an
    /// earlier command can also match; full-screen agent UIs need a screen model,
    /// add one with the terminal renderer.
    pub fn send(&mut self, text: &str, enter: bool, wait: Option<Duration>) -> Result<bool> {
        if text.contains(['\r', '\n']) {
            return Err(invalid("send text must be one line; use enter to submit"));
        }
        if self.read(0).closed {
            return Err(invalid("terminal is closed"));
        }
        self.writer.write_all(text.as_bytes())?;
        self.writer.flush()?;
        if !enter {
            self.pending |= !text.is_empty();
            return Ok(false);
        }
        let mark = self.end();
        self.writer.write_all(b"\r")?;
        self.writer.flush()?;
        self.pending = false;
        Ok(match wait {
            Some(timeout) => self.wait_for(mark, timeout, |b| b.contains(&b'\n')),
            None => false,
        })
    }

    /// True while text sent without Enter is waiting on the input line.
    pub fn has_pending_input(&self) -> bool {
        self.pending
    }

    /// Sends Ctrl-C to the foreground process; this also discards the input line.
    pub fn interrupt(&mut self) -> Result<()> {
        self.writer.write_all(b"\x03")?;
        self.writer.flush()?;
        self.pending = false;
        Ok(())
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        if rows == 0 || cols == 0 {
            return Err(invalid("terminal size must be positive"));
        }
        Ok(self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?)
    }

    /// Exit code once the process has ended.
    pub fn exit_code(&mut self) -> Result<Option<u32>> {
        Ok(self.child.try_wait()?.map(|status| status.exit_code()))
    }

    /// Stops the process and waits for it to exit. Processes it moved into
    /// other process groups are not tracked.
    pub fn close(mut self) -> Result<u32> {
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        Ok(self.child.wait()?.exit_code())
    }
}

impl Drop for Terminal {
    /// Dropping a terminal stops and reaps its process instead of leaking it.
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
