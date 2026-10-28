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

/// Scrollback kept in memory; older bytes are dropped but cursors stay absolute.
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
                let count = reader.read(&mut buffer).unwrap_or(0);
                let (lock, ready) = &*sink;
                let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
                if count == 0 {
                    state.closed = true;
                    ready.notify_all();
                    return;
                }
                state.bytes.extend_from_slice(&buffer[..count]);
                if state.bytes.len() > SCROLLBACK {
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
        })
    }

    /// Returns output from `cursor`; `next` is the cursor for the following read.
    pub fn read(&self, cursor: usize) -> Chunk {
        let state = self.output.0.lock().unwrap_or_else(|e| e.into_inner());
        let start = cursor.clamp(state.base, state.base + state.bytes.len());
        Chunk {
            text: String::from_utf8_lossy(&state.bytes[start - state.base..]).into_owned(),
            cursor: start,
            next: state.base + state.bytes.len(),
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
    /// shortcut: a line break is the cooked-mode echo of Enter; full-screen agent
    /// UIs need a screen model to confirm, add one with the terminal renderer.
    pub fn send(&mut self, text: &str, enter: bool, wait: Option<Duration>) -> Result<bool> {
        if text.contains(['\r', '\n']) {
            return Err(invalid("send text must be one line; use enter to submit"));
        }
        self.writer.write_all(text.as_bytes())?;
        self.writer.flush()?;
        if !enter {
            return Ok(false);
        }
        let mark = self.read(0).next;
        self.writer.write_all(b"\r")?;
        self.writer.flush()?;
        Ok(match wait {
            Some(timeout) => self.wait_for(mark, timeout, |b| b.contains(&b'\n')),
            None => false,
        })
    }

    /// Sends Ctrl-C to the foreground process.
    pub fn interrupt(&mut self) -> Result<()> {
        self.writer.write_all(b"\x03")?;
        Ok(self.writer.flush()?)
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

    /// Stops the process and waits for it to exit.
    pub fn close(mut self) -> Result<u32> {
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        Ok(self.child.wait()?.exit_code())
    }
}
