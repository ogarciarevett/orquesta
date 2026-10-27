// Copyright (c) 2026 Lovecast Inc.
// Copyright (c) 2026 Omar Garcia
// SPDX-License-Identifier: MIT
use orquesta_orchestration::{Mailbox, runtime::Runtime, status_bar::PROVIDERS};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
fn call(runtime: &Runtime, action: Value) -> Value {
    let request = json!({"version":1, "operation":{"command":"status-bar", "action":action}});
    let request = serde_json::from_value(request).unwrap();
    let response = runtime.dispatch(request).unwrap();
    response.result
}
fn refresh(runtime: &Runtime, id: &str, key: &str, result: (&str, u8)) -> Value {
    let begin = json!({"action":"begin", "id":id, "request_id":key});
    let claim = call(runtime, begin);
    let (phase, percent) = result;
    let windows = json!([{"label":"5h", "percent_used":percent}]);
    let finish = json!({"action":"finish", "id":id, "token":claim["token"], "now":100, "phase":phase, "windows":windows});
    call(runtime, finish)
}
#[test]
fn persistent_roster_settings_refresh_and_inventory() {
    let directory = std::env::temp_dir().join(format!("status-bar-{}", std::process::id()));
    let runtime = Runtime(Mailbox::open(&directory).unwrap());
    let invoke = |action| call(&runtime, action);
    let first = invoke(json!({"action":"snapshot"}));
    let providers = first["snapshot"]["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 10);
    for (provider, name) in providers.iter().zip(PROVIDERS) {
        assert_eq!(provider["name"], name);
    }
    assert_eq!(first["snapshot"]["providers"][0]["phase"], "loading");
    let first = invoke(json!({"action":"configure", "id":"local", "name":"Local decision"}));
    let cases = [
        ("gemini", "error_no_cache", 0),
        ("claude", "ready", 0),
        ("claude", "error_cached", 0),
        ("kimi", "off", 0),
        ("kimi", "not_signed_in", 0),
        ("claude", "stale", 0),
        ("decision:local", "ready", 80),
    ];
    for (id, phase, percent) in cases {
        let reply = refresh(&runtime, id, phase, (phase, percent));
        let providers = reply["snapshot"]["providers"].as_array().unwrap();
        let provider = providers.iter().find(|p| p["id"] == id).unwrap();
        assert_eq!(provider["phase"], phase);
        assert_eq!(providers.len(), 11);
        if id == "claude" {
            assert_eq!(provider["windows"][0]["percent_used"], 0.0);
        }
        if id.starts_with("decision:") {
            assert_eq!(reply["urgent"], json!([id]));
        }
    }
    let steady = refresh(&runtime, "decision:local", "steady", ("ready", 90));
    assert_eq!(steady["urgent"], json!([]));
    for name in PROVIDERS {
        let id = name.to_lowercase().replace(' ', "-");
        let reply = refresh(&runtime, &id, "fixture", ("ready", 42));
        assert_eq!(reply["snapshot"]["providers"].as_array().unwrap().len(), 11);
    }
    for kind in ["terminals", "ports", "hosts"] {
        for present in [true, true, false] {
            let action =
                json!({"action":"inventory", "kind":kind, "id":"entry", "present":present});
            assert_eq!(invoke(action)["counts"][kind], usize::from(present));
        }
    }
    for key in first["snapshot"]["visibility"].as_object().unwrap().keys() {
        invoke(json!({"action":"visibility", "key":key, "visible":false}));
    }
    invoke(json!({"action":"display", "mode":"detailed"}));
    let upstream = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                let action = json!({"action":"begin", "id":"codex", "request_id":"concurrent"});
                if call(&runtime, action)["started"] == true {
                    upstream.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
    });
    assert_eq!(upstream.load(Ordering::SeqCst), 1);
    let begin = json!({"action":"begin", "id":"codex", "request_id":"concurrent"});
    let claim = invoke(begin.clone());
    assert_eq!(claim["snapshot"]["providers"][1]["phase"], "refreshing");
    invoke(
        json!({"action":"finish", "id":"codex", "token":claim["token"], "now":200, "phase":"ready", "windows":[]}),
    );
    assert_eq!(invoke(begin)["started"], false);
    let persisted = call(
        &Runtime(Mailbox::open(&directory).unwrap()),
        json!({"action":"snapshot"}),
    );
    assert_eq!(persisted["snapshot"]["display_mode"], "detailed");
    assert_eq!(persisted["snapshot"]["providers"][1]["updated_at"], 200);
    for key in first["snapshot"]["visibility"].as_object().unwrap().keys() {
        assert_eq!(persisted["snapshot"]["visibility"][key], false);
    }
    std::fs::remove_dir_all(directory).unwrap();
}
