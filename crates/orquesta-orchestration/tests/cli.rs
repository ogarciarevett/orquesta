// Copyright (c) 2026 Lovecast Inc.
// SPDX-License-Identifier: MIT
use orquesta_orchestration::Mailbox;
use serde_json::{Value, json};
use std::process::Command;

#[test]
fn registry_validation_and_durable_command_roundtrip() {
    let directory = std::env::temp_dir().join(format!("orquesta-cli-{}", std::process::id()));
    let mailbox = Mailbox::open(&directory).unwrap();
    mailbox.register("run:r").unwrap();
    mailbox.register("session:s").unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_orquesta"))
            .args(if args[0] == "schema" {
                vec![]
            } else {
                vec!["orchestration"]
            })
            .args(args)
            .env("ORQUESTA_MAILBOX_DIR", &directory)
            .env("ORQUESTA_RUN_ID", "r")
            .env("ORQUESTA_FROM", "session:s")
            .output()
            .unwrap()
    };
    let call = |args: &[&str]| -> Value {
        let result = invoke(args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    };
    assert_eq!(call(&["schema"])["commands"].as_array().unwrap().len(), 6);
    for name in ["ask", "check", "inbox", "reply", "request-show", "send"] {
        assert!(call(&[name, "--help"])["flags"].is_array());
        assert!(!invoke(&[name, "--unknown"]).status.success());
    }
    for args in [
        vec!["send"],
        vec!["request-show"],
        vec!["ask", "--question", "x", "--resume", "1"],
        vec!["check", "--peek", "--all"],
        vec!["inbox", "extra"],
        vec!["inbox", "--full", "--full"],
    ] {
        assert!(!invoke(&args).status.success());
    }
    let sent = call(&["send", "--subject", "Hello", "--retry-request", "req"]);
    assert_eq!(call(&["request-show", "--request", "req"]), sent);
    assert_eq!(
        call(&["check", "--peek"])["messages"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let delivery = call(&["check"])["deliveryId"].to_string();
    call(&["check", "--ack", &delivery]);
    assert_eq!(call(&["check"])["messages"], json!([]));
    assert_eq!(call(&["inbox"])[0]["acknowledged"], true);
    let asked = call(&["ask", "--question", "Ready?", "--timeout-ms", "0"]);
    let id = asked["id"].to_string();
    call(&["reply", "--id", &id, "--from", "run:r", "--body", "Yes"]);
    assert_eq!(
        call(&["ask", "--resume", &id])["reply"]["envelope"]["body"],
        "Yes"
    );
    std::fs::remove_dir_all(directory).unwrap();
}
