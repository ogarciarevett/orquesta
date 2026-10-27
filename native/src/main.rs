// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
use orquesta_orchestration::{
    Mailbox,
    runtime::{Operation, Runtime},
    status_bar::{Action, DisplayMode},
    sidebar::Preference,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

const RECIPIENT: &str = "session:s";
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DesktopRequest {
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
        DesktopRequest::AddProject {} => return Err("Native picker required".into()),
        DesktopRequest::Sidebar {} => return runtime.0.sidebar_snapshot().and_then(|s| Ok(serde_json::to_value(s)?)).map_err(|e| e.to_string()),
        DesktopRequest::SidebarPreference { revision, patch } => return runtime.0.sidebar_preference(revision, patch).and_then(|s| Ok(serde_json::to_value(s)?)).map_err(|e| e.to_string()),
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
            let selected = app.dialog().file().set_parent(&window).set_title("Add project")
                .set_can_create_directories(false).blocking_pick_folder()
                .map(|path| path.into_path()).transpose().map_err(|e| e.to_string())?;
            return app.state::<Runtime>().0.sidebar_add_project(selected)
                .and_then(|s| Ok(serde_json::to_value(s)?)).map_err(|e| e.to_string());
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
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("orquesta-native-{nonce}"));
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
