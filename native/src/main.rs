// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
use orquesta_orchestration::{
    Mailbox,
    automations::AutomationDraft,
    runtime::{Operation, Runtime},
    sidebar::Preference,
    status_bar::{Action, DisplayMode},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

const RECIPIENT: &str = "session:s";
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DesktopRequest {
    Automations {},
    SaveAutomation { revision: u64, id: Option<String>, draft: AutomationDraft },
    AddProject {},
    Sidebar {},
    SidebarPreference { revision: u64, patch: Preference },
    Inbox {},
    Read {},
    Reconnect {},
    Ack { id: u64 },
    Status {},
    Visibility { key: String, visible: bool },
    Display { mode: DisplayMode },
    Refresh {},
}
fn dispatch(runtime: &Runtime, request: DesktopRequest) -> Result<Value, String> {
    let operation = match request {
        DesktopRequest::Automations {} => return sidebar_value(runtime.0.automation_snapshot()),
        DesktopRequest::SaveAutomation { revision, id, draft } => {
            return sidebar_value(runtime.0.automation_save(revision, id, draft));
        }
        DesktopRequest::AddProject {} => return Err("Native picker required".into()),
        DesktopRequest::Sidebar {} => return sidebar_value(runtime.0.sidebar_snapshot()),
        DesktopRequest::SidebarPreference { revision, patch } => {
            return sidebar_value(runtime.0.sidebar_preference(revision, patch));
        }
        DesktopRequest::Inbox {} => Operation::Inbox {
            recipient: Some(RECIPIENT.into()),
        },
        DesktopRequest::Read {} | DesktopRequest::Reconnect {} => Operation::Check {
            recipient: RECIPIENT.into(),
            peek: false,
        },
        DesktopRequest::Ack { id } => Operation::Ack {
            recipient: RECIPIENT.into(),
            id,
        },
        DesktopRequest::Status {} => Operation::StatusBar {
            action: Action::Snapshot,
        },
        DesktopRequest::Visibility { key, visible } => Operation::StatusBar {
            action: Action::Visibility { key, visible },
        },
        DesktopRequest::Display { mode } => Operation::StatusBar {
            action: Action::Display { mode },
        },
        DesktopRequest::Refresh {} => return Err("Usage refresh unavailable on this host".into()),
    };
    let mut result = runtime
        .call(operation)
        .map_err(|error| error.to_string())?
        .result;
    if result.get("snapshot").is_some() {
        result["refresh_providers"] = json!([]);
    }
    Ok(result)
}
fn sidebar_value(
    result: orquesta_orchestration::Result<impl serde::Serialize>,
) -> Result<Value, String> {
    let snapshot = result.map_err(|error| error.to_string())?;
    serde_json::to_value(snapshot).map_err(|error| error.to_string())
}
#[tauri::command]
async fn desktop_request(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    request: DesktopRequest,
) -> Result<Value, String> {
    if window.label() != "main" || !local_url(&window.url().map_err(|error| error.to_string())?) {
        return Err("Untrusted desktop caller".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        if matches!(request, DesktopRequest::AddProject {}) {
            let selected = app
                .dialog()
                .file()
                .set_parent(&window)
                .set_title("Add project")
                .set_can_create_directories(false)
                .blocking_pick_folder()
                .map(|path| path.into_path())
                .transpose()
                .map_err(|e| e.to_string())?;
            return sidebar_value(app.state::<Runtime>().0.sidebar_add_project(selected));
        }
        dispatch(app.state::<Runtime>().inner(), request)
    })
    .await
    .map_err(|error| error.to_string())?
}
fn local_url(url: &tauri::Url) -> bool {
    url.scheme() == "tauri" && url.host_str() == Some("localhost")
}
#[cfg(feature = "native-smoke")]
#[tauri::command]
fn smoke_complete(app: tauri::AppHandle, success: bool, detail: String) {
    // Reopen the same app-data mailbox through core, without an OS app relaunch.
    let reopened = (|| -> Result<(), String> {
        let directory = app.path().app_data_dir().map_err(|e| e.to_string())?.join("mailbox");
        let mailbox = Mailbox::open(directory).map_err(|e| e.to_string())?;
        let stored = sidebar_value(mailbox.automation_snapshot())?;
        let current = dispatch(app.state::<Runtime>().inner(), DesktopRequest::Automations {})?;
        if stored != current || stored["definitions"].as_array().is_none_or(Vec::is_empty) {
            return Err("Automation core reopen mismatch".into());
        }
        Ok(())
    })();
    let success = success && reopened.is_ok();
    let detail = match reopened {
        Ok(()) => detail,
        Err(error) => format!("{detail}; {error}"),
    };
    std::fs::write(
        "smoke-result.json",
        json!({"success":success,"detail":detail}).to_string(),
    )
    .expect("smoke report");
    app.exit(if success { 0 } else { 1 });
}
fn main() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    #[cfg(feature = "native-smoke")]
    let builder = builder.invoke_handler(tauri::generate_handler![desktop_request, smoke_complete]);
    #[cfg(not(feature = "native-smoke"))]
    let builder = builder.invoke_handler(tauri::generate_handler![desktop_request]);
    builder
        .setup(|app| {
            use std::os::unix::fs::PermissionsExt;
            let directory = app.path().app_data_dir()?.join("mailbox");
            std::fs::create_dir_all(&directory)?;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
            let mailbox = Mailbox::open(directory)?;
            mailbox.register(RECIPIENT)?;
            app.manage(Runtime(mailbox));
            let window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Orquesta")
            .inner_size(1280.0, 900.0)
            .on_navigation(local_url)
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny);
            #[cfg(feature = "native-smoke")]
            let window = window.on_page_load(|window, payload| {
                if payload.event() == tauri::webview::PageLoadEvent::Finished {
                    println!("NATIVE_WINDOW_LOADED");
                    window
                        .eval(include_str!("../smoke.js"))
                        .expect("smoke injection");
                }
            });
            window.build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Orquesta desktop startup failed");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persisted_inbox_boundary_and_corruption() {
        let fixture = Fixture::new();
        let directory = fixture.0.clone();
        let mailbox = Mailbox::open(&directory).unwrap();
        mailbox.register(RECIPIENT).unwrap();
        mailbox.register("session:other").unwrap();
        let envelope = orquesta_orchestration::Envelope {
            request_id: "native-test".into(),
            from: "session:other".into(),
            to: RECIPIENT.into(),
            kind: "notice".into(),
            subject: "Inbox test".into(),
            body: "Separate from draft".into(),
            thread: None,
            metadata: Default::default(),
        };
        let message = mailbox.send(envelope).unwrap();
        let runtime = Runtime(mailbox);
        let inbox = dispatch(&runtime, DesktopRequest::Inbox {}).unwrap();
        assert_eq!(inbox.as_array().unwrap().len(), 1);
        let sidebar = dispatch(&runtime, DesktopRequest::Sidebar {}).unwrap();
        assert_eq!(sidebar["projects"], json!([]));
        runtime
            .0
            .sidebar_add_project(Some(directory.clone()))
            .unwrap();
        let sidebar = dispatch(&runtime, DesktopRequest::Sidebar {}).unwrap();
        let selected = sidebar["projects"][0]["workspace_id"].as_str().unwrap();
        dispatch(
            &runtime,
            DesktopRequest::SidebarPreference {
                revision: sidebar["revision"].as_u64().unwrap(),
                patch: Preference::Select {
                    workspace_id: selected.into(),
                },
            },
        )
        .unwrap();
        assert_eq!(
            dispatch(&runtime, DesktopRequest::Inbox {}).unwrap()[0]["read"],
            false
        );
        dispatch(&runtime, DesktopRequest::Read {}).unwrap();
        assert_eq!(
            dispatch(&runtime, DesktopRequest::Reconnect {}).unwrap()[0]["id"],
            message.id
        );
        dispatch(&runtime, DesktopRequest::Ack { id: message.id }).unwrap();
        assert_eq!(
            dispatch(&runtime, DesktopRequest::Reconnect {}).unwrap(),
            json!([])
        );
        dispatch(
            &runtime,
            DesktopRequest::Visibility {
                key: "ports".into(),
                visible: false,
            },
        )
        .unwrap();
        let reopened = Runtime(Mailbox::open(&directory).unwrap());
        assert_eq!(
            dispatch(&reopened, DesktopRequest::Status {}).unwrap()["snapshot"]["visibility"]["ports"],
            false
        );
        std::fs::write(directory.join("mailbox.json"), "corrupt").unwrap();
        assert!(dispatch(&reopened, DesktopRequest::Status {}).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            loop {
                let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
                let nonce = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let root = std::env::temp_dir().join(format!(
                    "native-draft-{}-{nonce}-{sequence}", std::process::id()
                ));
                match std::fs::create_dir(&root) {
                    Ok(()) => return Self(root),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("fixture: {error}"),
                }
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn named(runtime: &Runtime, request: Value) -> Result<Value, String> {
        dispatch(runtime, serde_json::from_value(request).unwrap())
    }
    fn save_request() -> Value {
        json!({"kind":"save_automation", "revision":0, "id":null, "draft":{
            "name":"Review", "prompt":" \nReview λ\n  keep indentation\n",
            "project_id":"project-1", "workspace_id":"folder-1",
            "schedule":{"kind":"interval", "every_seconds":60, "anchor_utc_seconds":0}
        }})
    }
    #[test]
    fn named_automation_dispatch_persists_and_isolates_failures() {
        let fixture = Fixture::new();
        let runtime = Runtime(Mailbox::open(&fixture.0).unwrap());
        runtime.0.register(RECIPIENT).unwrap();
        runtime.0.send(orquesta_orchestration::Envelope {
            request_id: "automation-unread".into(), from: RECIPIENT.into(),
            to: RECIPIENT.into(), kind: "notice".into(), subject: "Keep".into(),
            body: "Unread".into(), thread: None, metadata: Default::default(),
        }).unwrap();
        for name in ["first", "second"] {
            let folder = fixture.0.join(name);
            std::fs::create_dir(&folder).unwrap();
            runtime.0.sidebar_add_project(Some(folder)).unwrap();
        }
        dispatch(&runtime, DesktopRequest::Display { mode: DisplayMode::Detailed }).unwrap();
        runtime.0.sidebar_preference(2, Preference::Select {
            workspace_id: "folder-1".into(),
        }).unwrap();
        let state = || -> Value {
            serde_json::from_slice(&std::fs::read(fixture.0.join("mailbox.json")).unwrap()).unwrap()
        };
        let mut unrelated = state();
        unrelated.as_object_mut().unwrap().remove("automations");
        let request = save_request();
        let saved = named(&runtime, request.clone()).unwrap();
        assert_eq!(saved["revision"], 1);
        assert_eq!(saved["definitions"][0]["id"], "automation-1");
        assert_eq!(saved["definitions"][0]["draft"], request["draft"]);
        let reopened = Runtime(Mailbox::open(&fixture.0).unwrap());
        assert_eq!(named(&reopened, json!({"kind":"automations"})).unwrap(), saved);
        let bytes = std::fs::read(fixture.0.join("mailbox.json")).unwrap();
        assert_eq!(named(&runtime, request.clone()).unwrap_err(),
            "automations changed; reload snapshot before retrying");
        for (pointer, value, error) in [
            ("/id", json!("automation-999"), "unknown automation ID"),
            ("/draft/project_id", json!("foreign"), "unknown automation project/workspace pair"),
            ("/draft/workspace_id", json!("folder-2"), "unknown automation project/workspace pair"),
            ("/draft/prompt", json!("  "), "invalid automation draft; check name, prompt and interval limits"),
        ] {
            let mut invalid = request.clone();
            invalid["revision"] = json!(1);
            *invalid.pointer_mut(pointer).unwrap() = value;
            assert_eq!(named(&runtime, invalid).unwrap_err(), error);
            assert_eq!(std::fs::read(fixture.0.join("mailbox.json")).unwrap(), bytes);
        }
        std::fs::create_dir(fixture.0.join("mailbox.pending")).unwrap();
        let mut replacement = request.clone();
        replacement["revision"] = json!(1);
        replacement["id"] = json!("automation-1");
        replacement["draft"]["prompt"] = json!("Replacement");
        assert!(named(&runtime, replacement.clone()).is_err());
        assert_eq!(std::fs::read(fixture.0.join("mailbox.json")).unwrap(), bytes);
        std::fs::remove_dir(fixture.0.join("mailbox.pending")).unwrap();
        assert_eq!(named(&reopened, json!({"kind":"automations"})).unwrap(), saved);
        let updated = named(&runtime, replacement).unwrap();
        assert_eq!(updated["revision"], 2);
        assert_eq!(updated["definitions"][0]["id"], "automation-1");
        assert_eq!(updated["definitions"][0]["draft"]["prompt"], "Replacement");
        let mut after = state();
        after.as_object_mut().unwrap().remove("automations");
        assert_eq!(after, unrelated);
        assert_eq!(dispatch(&runtime, DesktopRequest::Inbox {}).unwrap()[0]["read"], false);
    }
    #[test]
    fn automation_requests_reject_unknown_and_invalid_shapes() {
        let request = save_request();
        for field in ["run", "enable", "delete", "recipient", "path", "cwd", "process",
            "executable", "provider", "env", "host", "session", "precheck", "args", "status"] {
            for pointer in ["", "/draft", "/draft/schedule"] {
                let mut invalid = request.clone();
                invalid.pointer_mut(pointer).unwrap()[field] = json!("forbidden");
                assert!(serde_json::from_value::<DesktopRequest>(invalid).is_err(), "{pointer}/{field}");
            }
            assert!(serde_json::from_value::<DesktopRequest>(json!({"kind":"automations",(field):true})).is_err());
        }
        for kind in ["run", "enable", "delete", "automation_run", "automation_enable", "automation_delete"] {
            assert!(serde_json::from_value::<DesktopRequest>(json!({"kind":kind})).is_err());
        }
        for pointer in ["/revision", "/draft", "/draft/name", "/draft/prompt",
            "/draft/project_id", "/draft/workspace_id", "/draft/schedule",
            "/draft/schedule/kind", "/draft/schedule/every_seconds", "/draft/schedule/anchor_utc_seconds"] {
            let mut missing = request.clone();
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            missing.pointer_mut(parent).unwrap().as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<DesktopRequest>(missing).is_err(), "{pointer}");
        }
        for (pointer, value) in [("/revision", json!(-1)), ("/id", json!(4)),
            ("/draft/name", json!(false)), ("/draft/schedule/kind", json!("cron")),
            ("/draft/schedule/every_seconds", json!("60"))] {
            let mut invalid = request.clone();
            *invalid.pointer_mut(pointer).unwrap() = value;
            assert!(serde_json::from_value::<DesktopRequest>(invalid).is_err(), "{pointer}");
        }
    }
    #[test]
    fn rejects_webview_authority_expansion() {
        for input in [
            r#"{"kind":"add_project","path":"/tmp"}"#,
            r#"{"kind":"sidebar_preference","revision":0,"patch":{"action":"select","workspace_id":"folder-1","path":"/tmp"}}"#,
            r#"{"kind":"sidebar","executable":"git"}"#,
            r#"{"kind":"send"}"#,
            r#"{"kind":"inbox","recipient":"session:other"}"#,
            r#"{"kind":"status","path":"/tmp"}"#,
            r#"{"kind":"finish","windows":[]}"#,
        ] {
            assert!(serde_json::from_str::<DesktopRequest>(input).is_err());
        }
        assert!(!local_url(&"https://example.com".parse().unwrap()));
        assert!(local_url(&"tauri://localhost/index.html".parse().unwrap()));
    }
}
