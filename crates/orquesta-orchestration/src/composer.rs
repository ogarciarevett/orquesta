// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
use crate::{
    Message, Result, TerminalDraftBuffer, invalid,
    runtime::{MailEvent, Operation, Runtime},
};

pub struct SessionComposer {
    recipient: String,
    draft: TerminalDraftBuffer,
    pub inbox: Vec<Message>,
    pub notifications: Vec<MailEvent>,
    connected: bool,
    submissions: u64,
}
impl SessionComposer {
    pub fn new(recipient: String) -> Self {
        Self {
            recipient,
            draft: TerminalDraftBuffer {
                text: String::new(),
                cursor: 0,
                selection: None,
            },
            inbox: Vec::new(),
            notifications: Vec::new(),
            connected: false,
            submissions: 0,
        }
    }
    pub fn draft(&self) -> &TerminalDraftBuffer {
        &self.draft
    }
    pub fn submissions(&self) -> u64 {
        self.submissions
    }
    pub fn select(&mut self, cursor: usize, selection: Option<(usize, usize)>) -> Result<()> {
        let valid = |p| self.draft.text.is_char_boundary(p);
        if !valid(cursor) || selection.is_some_and(|(a, b)| a > b || !valid(a) || !valid(b)) {
            return Err(invalid("invalid cursor or selection"));
        }
        self.draft.cursor = cursor;
        self.draft.selection = selection;
        Ok(())
    }
    pub fn edit(&mut self, text: &str) {
        let (start, end) = self
            .draft
            .selection
            .unwrap_or((self.draft.cursor, self.draft.cursor));
        self.draft.text.replace_range(start..end, text);
        self.draft.cursor = start + text.len();
        self.draft.selection = None;
    }
    pub fn submit(&mut self) -> String {
        self.submissions += 1;
        self.draft.cursor = 0;
        self.draft.selection = None;
        std::mem::take(&mut self.draft.text)
    }
    pub fn disconnect(&mut self) {
        self.connected = false;
    }
    pub fn reconnect(&mut self, runtime: &Runtime) -> Result<()> {
        let messages = runtime.replay(&self.recipient)?;
        self.inbox = messages;
        self.connected = true;
        Ok(())
    }
    pub fn notify(&mut self, event: MailEvent) {
        if self.connected
            && event.recipient == self.recipient
            && !self.notifications.contains(&event)
        {
            self.notifications.push(event);
        }
    }
    pub fn ack(&mut self, runtime: &Runtime, id: u64) -> Result<()> {
        runtime.call(Operation::Ack {
            recipient: self.recipient.clone(),
            id,
        })?;
        self.inbox.retain(|m| m.id != id);
        self.notifications.retain(|e| e.id != id);
        Ok(())
    }
}
