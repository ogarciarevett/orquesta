// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
use orquesta_orchestration::{
    Mailbox,
    composer::SessionComposer,
    runtime::{Response, Runtime},
};
use serde_json::{Value, json};

#[test]
fn ipc_delivery_preserves_editable_draft_across_reconnect() -> orquesta_orchestration::Result<()> {
    let directory = std::env::temp_dir().join(format!("orquesta-runtime-{}", std::process::id()));
    let runtime = Runtime(Mailbox::open(&directory)?);
    for address in ["run:r", "dispatch:d", "session:s"] {
        runtime.0.register(address)?;
    }
    let call = |operation: Value| -> orquesta_orchestration::Result<Response> {
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_orquesta"))
            .arg("ipc")
            .env("ORQUESTA_MAILBOX_DIR", &directory)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        serde_json::to_writer(
            child.stdin.take().unwrap(),
            &json!({"version": 1, "operation": operation}),
        )?;
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(std::io::Error::other(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            )
            .into());
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let mut composer = SessionComposer::new("session:s".into());
    composer.edit("first line\nsecond λ line\nlast line");
    composer.select(14, Some((12, 17)))?;
    composer.edit("edited");
    composer.select(14, Some((12, 16)))?;
    let draft = format!("{:?}", composer.draft());
    composer.reconnect(&runtime)?;
    for id in 1..=3 {
        let envelope = json!({"request_id": format!("req-{id}"), "to": "session:s", "from": "run:r", "kind": "status", "subject": "Update", "body": "Inbox only", "thread": null});
        let operation = json!({"command": "send", "envelope": envelope});
        let response = call(operation.clone())?;
        assert_eq!(response.events.len(), 1);
        composer.notify(response.events[0].clone());
        composer.notify(response.events[0].clone());
        assert_eq!(format!("{:?}", composer.draft()), draft);
        assert_eq!(composer.submissions(), 0);
        assert!(call(operation)?.events.is_empty());
    }
    assert_eq!(composer.notifications.len(), 3);
    assert_eq!(
        call(json!({"command": "check", "recipient": "session:s", "peek": true}))?
            .result
            .as_array()
            .unwrap()
            .len(),
        3
    );
    composer.reconnect(&runtime)?;
    composer.disconnect();
    composer.reconnect(&Runtime(Mailbox::open(&directory)?))?;
    composer.reconnect(&runtime)?;
    assert_eq!(
        composer.inbox.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(call(json!({"command": "ack", "recipient": "dispatch:d", "id": 1})).is_err());
    composer.ack(&runtime, 1)?;
    composer.reconnect(&runtime)?;
    assert_eq!(
        composer.inbox.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(
        call(json!({"command": "inbox", "recipient": "session:s"}))?
            .result
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        call(json!({"command": "request-show", "request_id": "req-1"}))?.result["id"],
        1
    );
    let question = json!({"request_id": "question", "to": "dispatch:d", "from": "run:r", "kind": "question", "subject": "Ready?", "body": "", "thread": null});
    let asked = call(json!({"command": "ask", "envelope": question.clone()}))?;
    let reply = call(
        json!({"command": "reply", "recipient": "dispatch:d", "id": asked.result["id"], "request_id": "answer", "body": "Yes"}),
    )?;
    assert_eq!(reply.events[0].recipient, "run:r");
    assert_eq!(reply.result["envelope"]["thread"], asked.result["id"]);
    let mut invalid = question;
    invalid["to"] = json!("session:unknown");
    assert!(call(json!({"command": "send", "envelope": invalid})).is_err());
    let request = serde_json::from_value(
        json!({"version": 2, "operation": {"command": "inbox", "recipient": null}}),
    )?;
    assert!(runtime.dispatch(request).is_err());
    assert_eq!(format!("{:?}", composer.draft()), draft);
    assert_eq!(composer.submissions(), 0);
    assert!(composer.select(usize::MAX, None).is_err());
    assert!(
        serde_json::from_value::<orquesta_orchestration::runtime::Request>(
            json!({"version": 1, "operation": {"command": "unknown"}})
        )
        .is_err()
    );
    assert!(!composer.submit().is_empty());
    assert_eq!(composer.submissions(), 1);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
