// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
use crate::{Envelope, Mailbox, Message, Result, invalid};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const VERSION: u16 = 1;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u16,
    pub operation: Operation,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Operation {
    StatusBar {
        action: crate::status_bar::Action,
    },
    Send {
        envelope: Envelope,
    },
    Ask {
        envelope: Envelope,
    },
    Check {
        recipient: String,
        peek: bool,
    },
    Inbox {
        recipient: Option<String>,
    },
    Ack {
        recipient: String,
        id: u64,
    },
    Reply {
        recipient: String,
        id: u64,
        request_id: String,
        body: String,
    },
    RequestShow {
        request_id: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MailEvent {
    pub id: u64,
    pub recipient: String,
}
#[derive(Serialize, Deserialize)]
pub struct Response {
    pub version: u16,
    pub result: Value,
    pub events: Vec<MailEvent>,
}
pub struct Runtime(pub Mailbox);
impl Runtime {
    pub fn dispatch(&self, request: Request) -> Result<Response> {
        if request.version != VERSION {
            return Err(invalid("unsupported protocol version"));
        }
        let mut events = Vec::new();
        let mut deliver = |envelope| -> Result<Value> {
            let (message, created) = self.0.send_receipt(envelope)?;
            if created {
                events.push(MailEvent {
                    id: message.id,
                    recipient: message.envelope.to.clone(),
                });
            }
            Ok(json!(message))
        };
        let result = match request.operation {
            Operation::StatusBar { action } => self.0.status_bar(action)?,
            Operation::Send { envelope } => deliver(envelope)?,
            Operation::Ask { mut envelope } => {
                envelope.kind = "question".into();
                deliver(envelope)?
            }
            Operation::Check { recipient, peek } => json!(self.0.check(&recipient, peek)?),
            Operation::Inbox { recipient } => json!(self.0.inbox(recipient.as_deref())?),
            Operation::Ack { recipient, id } => {
                self.0.ack(&recipient, id)?;
                json!({"acknowledged": id})
            }
            Operation::Reply {
                recipient,
                id,
                request_id,
                body,
            } => {
                let original = self
                    .0
                    .inbox(Some(&recipient))?
                    .into_iter()
                    .find(|m| m.id == id)
                    .ok_or_else(|| invalid("message not in recipient inbox"))?;
                deliver(Envelope {
                    request_id,
                    to: original.envelope.from,
                    from: recipient,
                    kind: "reply".into(),
                    subject: original.envelope.subject,
                    body,
                    thread: Some(id),
                    metadata: Default::default(),
                })?
            }
            Operation::RequestShow { request_id } => json!(
                self.0
                    .inbox(None)?
                    .into_iter()
                    .find(|m| m.envelope.request_id == request_id)
            ),
        };
        Ok(Response {
            version: VERSION,
            result,
            events,
        })
    }
    pub fn call(&self, operation: Operation) -> Result<Response> {
        self.dispatch(Request {
            version: VERSION,
            operation,
        })
    }
    pub fn replay(&self, recipient: &str) -> Result<Vec<Message>> {
        Ok(serde_json::from_value(
            self.call(Operation::Check {
                recipient: recipient.into(),
                peek: false,
            })?
            .result,
        )?)
    }
}
