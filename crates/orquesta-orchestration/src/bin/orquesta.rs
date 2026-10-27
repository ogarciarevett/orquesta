// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
use orquesta_orchestration::{
    Envelope, Mailbox, Result,
    runtime::{Request, Runtime},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MESSAGE_TYPES: &str =
    "status,dispatch,worker_done,merge_ready,escalation,handoff,decision_gate,question,heartbeat";
const GLOBAL: &str = "help,json,pairing-code,environment";
const COMMANDS: &[(&str, &str)] = &[
    (
        "ask",
        "to,run,question,resume,dispatch-capability,options,timeout-ms,from,retry-request",
    ),
    (
        "check",
        "terminal,run,ack,unread,peek,all,types,format,wait,timeout-ms,retry-request",
    ),
    ("inbox", "limit,terminal,full"),
    ("reply", "id,body,run,from,retry-request"),
    ("request-show", "request"),
    (
        "send",
        "to,run,from,subject,body,type,priority,thread-id,payload,task-id,dispatch-id,dispatch-capability,retry-request,outcome,files-modified,report-path,phase",
    ),
];
const SWITCHES: &[&str] = &[
    "help", "json", "unread", "peek", "all", "format", "wait", "full",
];
fn error(text: &str) -> Box<dyn std::error::Error> {
    io::Error::new(io::ErrorKind::InvalidInput, text).into()
}
fn usage(command: &str) -> Value {
    serde_json::from_str::<Value>(include_str!("../usage.json")).unwrap()[command].clone()
}
fn registry() -> Value {
    json!({"version": 1, "messageTypes": MESSAGE_TYPES.split(',').collect::<Vec<_>>(), "commands": COMMANDS.iter().map(|(name, flags)| {
        json!({"path": ["orchestration", name], "flags": format!("{GLOBAL},{flags}").split(',').collect::<Vec<_>>(),
            "usage": usage(name), "aliases": [], "positionals": [], "repeatable": []})
    }).collect::<Vec<_>>()})
}
fn run(args: Vec<String>) -> Result<Value> {
    if args == ["ipc"] {
        let directory = std::env::var("ORQUESTA_MAILBOX_DIR")?;
        let request: Request = serde_json::from_reader(std::io::stdin())?;
        return Ok(json!(Runtime(Mailbox::open(directory)?).dispatch(request)?));
    }
    if args == ["schema"] {
        return Ok(registry());
    }
    if args.len() < 2 || args[0] != "orchestration" {
        return Err(error(
            "expected orchestration and one of ask, check, inbox, reply, request-show, send",
        ));
    }
    let command = args[1].as_str();
    let (_, allowed) = COMMANDS
        .iter()
        .find(|(name, _)| *name == command)
        .ok_or_else(|| error("unknown command"))?;
    let allowed = format!("{GLOBAL},{allowed}");
    let mut flags = BTreeMap::new();
    let mut iter = args[2..].iter();
    while let Some(flag) = iter.next() {
        let name = flag
            .strip_prefix("--")
            .ok_or_else(|| error("unexpected positional argument"))?;
        if !allowed.split(',').any(|f| f == name) {
            return Err(error("unknown flag"));
        }
        let value = if SWITCHES.contains(&name) {
            "true"
        } else {
            iter.next()
                .filter(|v| !v.starts_with("--"))
                .ok_or_else(|| error("missing flag value"))?
        };
        if flags.insert(name, value).is_some() {
            return Err(error("repeated flag"));
        }
    }
    if flags.contains_key("help") {
        return Ok(
            json!({"usage": usage(command), "flags": allowed.split(',').collect::<Vec<_>>()}),
        );
    }
    let get = |name: &str| flags.get(name).copied();
    let required = |name: &str| get(name).ok_or_else(|| error(&format!("missing {name}")));
    if get("pairing-code").is_some() || get("dispatch-capability").is_some() {
        return Err(error("remote capabilities require a connected runtime"));
    }
    if ["unread", "peek", "all"]
        .iter()
        .filter(|f| get(f).is_some())
        .count()
        > 1
    {
        return Err(error("unread, peek, and all are mutually exclusive"));
    }
    let scope = get("environment").unwrap_or("local");
    if scope != "local" {
        return Err(error("only the local environment is connected"));
    }
    let directory = std::env::var("ORQUESTA_MAILBOX_DIR")
        .map_err(|_| error("ORQUESTA_MAILBOX_DIR is required"))?;
    let mailbox = Mailbox::open(directory)?;
    let bound_run = std::env::var("ORQUESTA_RUN_ID").ok();
    let bound_sender = std::env::var("ORQUESTA_FROM").ok();
    let address = |raw: &str| {
        if raw.contains(':') {
            raw.to_owned()
        } else {
            format!("session:{raw}")
        }
    };
    let recipient = get("terminal").map(address).or_else(|| {
        get("run")
            .or(bound_run.as_deref())
            .map(|r| format!("run:{r}"))
    });
    let sender = || {
        get("from")
            .or(bound_sender.as_deref())
            .map(address)
            .ok_or_else(|| error("sender binding required"))
    };
    let request = || {
        get("retry-request").map(str::to_owned).unwrap_or_else(|| {
            format!(
                "{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
        })
    };
    let timeout = Duration::from_millis(get("timeout-ms").unwrap_or("30000").parse()?);
    match command {
        "send" | "ask" if get("resume").is_none() => {
            let question = command == "ask";
            let subject = required(if question { "question" } else { "subject" })?;
            let to = get("to")
                .map(address)
                .or(recipient)
                .ok_or_else(|| error("recipient binding required"))?;
            let kind = if question {
                "question"
            } else {
                get("type").unwrap_or("status")
            };
            if !MESSAGE_TYPES.split(',').any(|t| t == kind) {
                return Err(error("unknown message type"));
            }
            let mut metadata = BTreeMap::new();
            for key in [
                "priority",
                "task-id",
                "dispatch-id",
                "outcome",
                "files-modified",
                "report-path",
                "phase",
                "options",
            ] {
                if let Some(value) = get(key) {
                    metadata.insert(key.into(), json!(value));
                }
            }
            if let Some(payload) = get("payload") {
                metadata.insert("payload".into(), serde_json::from_str(payload)?);
            }
            if get("outcome").is_some_and(|v| !["succeeded", "failed"].contains(&v))
                || (kind == "worker_done" && get("outcome").is_none())
            {
                return Err(error("worker outcome must be succeeded or failed"));
            }
            let message = mailbox.send(Envelope {
                request_id: request(),
                to,
                from: sender()?,
                kind: kind.into(),
                subject: subject.into(),
                body: get("body").unwrap_or("").into(),
                thread: get("thread-id").map(str::parse).transpose()?,
                metadata,
            })?;
            if question {
                wait_reply(&mailbox, message.id, timeout)
            } else {
                Ok(json!(message))
            }
        }
        "ask" => {
            if get("question").is_some() {
                return Err(error("question and resume are mutually exclusive"));
            }
            let id = required("resume")?.parse()?;
            let original = mailbox
                .inbox(None)?
                .into_iter()
                .find(|m| m.id == id && m.envelope.from == sender().unwrap_or_default())
                .ok_or_else(|| error("question not found for sender"))?;
            if original.envelope.kind != "question" {
                return Err(error("resume requires a question"));
            }
            wait_reply(&mailbox, id, timeout)
        }
        "reply" => Ok(json!(mailbox.reply(
            &sender()?,
            required("id")?.parse()?,
            &request(),
            required("body")?
        )?)),
        "request-show" => {
            let request_id = required("request")?;
            Ok(json!(
                mailbox
                    .inbox(None)?
                    .into_iter()
                    .find(|m| m.envelope.request_id == request_id)
            ))
        }
        "inbox" => Ok(json!(
            mailbox
                .inbox(recipient.as_deref())?
                .into_iter()
                .take(get("limit").unwrap_or("100").parse()?)
                .collect::<Vec<_>>()
        )),
        "check" => {
            let to = recipient.ok_or_else(|| error("recipient binding required"))?;
            if let Some(id) = get("ack") {
                let id: u64 = id.parse()?;
                let batch: Vec<_> = mailbox
                    .inbox(Some(&to))?
                    .into_iter()
                    .filter(|m| m.id <= id && m.read)
                    .collect();
                if !batch.iter().any(|m| m.id == id) {
                    return Err(error("unknown delivery ID"));
                }
                for message in batch {
                    mailbox.ack(&to, message.id)?;
                }
            }
            let start = Instant::now();
            let mut keepalive = Instant::now();
            loop {
                let inspect = get("all").is_some() || get("peek").is_some();
                let mut rows = mailbox.inbox(Some(&to))?;
                if get("all").is_none() {
                    rows.retain(|m| !m.acknowledged);
                }
                if get("peek").is_some() || get("unread").is_some() {
                    rows.retain(|m| !m.read);
                }
                let matches = |m: &orquesta_orchestration::Message| {
                    get("types").is_none_or(|types| types.split(',').any(|t| t == m.envelope.kind))
                };
                if inspect {
                    rows.retain(matches);
                }
                let ready = rows.iter().any(matches);
                if ready || get("wait").is_none() || start.elapsed() >= timeout {
                    if !inspect && (ready || get("wait").is_none()) {
                        rows = mailbox.check(&to, false)?;
                    }
                    if !ready && get("wait").is_some() {
                        rows.clear();
                    }
                    return Ok(json!({"deliveryId": rows.last().map(|m| m.id), "messages": rows}));
                }
                if keepalive.elapsed() >= Duration::from_secs(15) {
                    eprintln!("{{\"_keepalive\":true}}");
                    keepalive = Instant::now();
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        _ => Err(error("invalid command")),
    }
}
fn wait_reply(mailbox: &Mailbox, id: u64, timeout: Duration) -> Result<Value> {
    let start = Instant::now();
    loop {
        if let Some(reply) = mailbox
            .inbox(None)?
            .into_iter()
            .find(|m| m.envelope.thread == Some(id) && m.envelope.kind == "reply")
        {
            return Ok(json!({"id": id, "reply": reply}));
        }
        if start.elapsed() >= timeout {
            return Ok(json!({"id": id, "timedOut": true}));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn main() {
    match run(std::env::args().skip(1).collect()) {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap()),
        Err(error) => {
            eprintln!(
                "{}",
                json!({"error": {"code": "invalid_request", "message": error.to_string()}})
            );
            std::process::exit(1);
        }
    }
}
