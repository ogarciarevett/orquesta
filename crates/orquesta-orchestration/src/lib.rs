// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::PathBuf,
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn invalid(message: &str) -> Box<dyn std::error::Error> {
    io::Error::new(io::ErrorKind::InvalidInput, message).into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Envelope {
    pub request_id: String,
    pub to: String,
    pub from: String,
    pub kind: String,
    pub subject: String,
    pub body: String,
    pub thread: Option<u64>,
    #[serde(default)]
    pub metadata: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub id: u64,
    pub envelope: Envelope,
    pub read: bool,
    pub acknowledged: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    recipients: Vec<String>,
    messages: Vec<Message>,
}

pub struct Mailbox(PathBuf);
impl Mailbox {
    pub fn open(directory: impl Into<PathBuf>) -> Result<Self> {
        let directory = directory.into();
        fs::create_dir_all(&directory)?;
        Ok(Self(directory))
    }

    fn transaction<T>(&self, action: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.0.join("lock"))?;
        lock.lock()?;
        let path = self.0.join("mailbox.json");
        let mut state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => State::default(),
            Err(error) => return Err(error.into()),
        };
        let result = action(&mut state)?;
        let temporary = self.0.join("mailbox.pending");
        let mut file = File::create(&temporary)?;
        serde_json::to_writer(&mut file, &state)?;
        file.sync_all()?;
        fs::rename(temporary, path)?;
        #[cfg(unix)]
        File::open(&self.0)?.sync_all()?;
        Ok(result)
    }

    pub fn register(&self, address: &str) -> Result<()> {
        let (scope, id) = address
            .split_once(':')
            .ok_or_else(|| invalid("invalid address"))?;
        if !matches!(scope, "run" | "dispatch" | "session")
            || id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(invalid("invalid address"));
        }
        self.transaction(|state| {
            if !state.recipients.iter().any(|a| a == address) {
                state.recipients.push(address.into());
            }
            Ok(())
        })
    }

    pub fn send(&self, envelope: Envelope) -> Result<Message> {
        self.send_receipt(envelope).map(|(message, _)| message)
    }

    pub fn send_receipt(&self, envelope: Envelope) -> Result<(Message, bool)> {
        self.transaction(|state| {
            if envelope.request_id.is_empty()
                || !state.recipients.contains(&envelope.to)
                || !state.recipients.contains(&envelope.from)
            {
                return Err(invalid(
                    "request ID and registered sender/recipient required",
                ));
            }
            if let Some(prior) = state
                .messages
                .iter()
                .find(|m| m.envelope.request_id == envelope.request_id)
            {
                return if prior.envelope == envelope {
                    Ok((prior.clone(), false))
                } else {
                    Err(invalid("request ID conflict"))
                };
            }
            let message = Message {
                id: state.messages.len() as u64 + 1,
                envelope,
                read: false,
                acknowledged: false,
            };
            state.messages.push(message.clone());
            Ok((message, true))
        })
    }

    pub fn inbox(&self, recipient: Option<&str>) -> Result<Vec<Message>> {
        self.transaction(|state| {
            Ok(state
                .messages
                .iter()
                .filter(|m| recipient.is_none_or(|r| m.envelope.to == r))
                .cloned()
                .collect())
        })
    }

    pub fn check(&self, recipient: &str, peek: bool) -> Result<Vec<Message>> {
        self.transaction(|state| {
            let replay = state
                .messages
                .iter()
                .any(|m| m.envelope.to == recipient && m.read && !m.acknowledged);
            let mut messages = Vec::new();
            for message in &mut state.messages {
                if message.envelope.to == recipient
                    && !message.acknowledged
                    && (!peek || !message.read)
                    && (peek || !replay || message.read)
                {
                    messages.push(message.clone());
                    if !peek {
                        message.read = true;
                    }
                }
            }
            Ok(messages)
        })
    }

    pub fn ack(&self, recipient: &str, id: u64) -> Result<()> {
        self.transaction(|state| {
            let message = state
                .messages
                .iter_mut()
                .find(|m| m.id == id && m.envelope.to == recipient)
                .ok_or_else(|| invalid("message not in recipient inbox"))?;
            message.acknowledged = true;
            message.read = true;
            Ok(())
        })
    }

    pub fn reply(&self, recipient: &str, id: u64, request_id: &str, body: &str) -> Result<Message> {
        let original = self
            .inbox(Some(recipient))?
            .into_iter()
            .find(|m| m.id == id)
            .ok_or_else(|| invalid("message not in recipient inbox"))?;
        self.send(Envelope {
            request_id: request_id.into(),
            to: original.envelope.from,
            from: recipient.into(),
            kind: "reply".into(),
            subject: original.envelope.subject,
            body: body.into(),
            thread: Some(id),
            metadata: Default::default(),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TerminalDraftBuffer {
    pub text: String,
    pub cursor: usize,
    pub selection: Option<(usize, usize)>,
}

#[cfg(test)]
mod tests;

pub mod composer;
pub mod runtime;
