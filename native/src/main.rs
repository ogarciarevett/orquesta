// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
use orquesta_orchestration::{Mailbox, runtime::{Operation, Runtime}, status_bar::{Action, DisplayMode}};
use serde::Deserialize;
use serde_json::{Value, json};
use tauri::Manager;

const RECIPIENT: &str = "session:s";
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DesktopRequest {
    Inbox,
    Read,
    Reconnect,
    Ack { id: u64 },
    Status,
    Visibility { key: String, visible: bool },
    Display { mode: DisplayMode },
    Refresh,
}
fn dispatch(runtime: &Runtime, request: DesktopRequest) -> Result<Value, String> {
    let operation = match request {
        DesktopRequest::Inbox => Operation::Inbox { recipient: Some(RECIPIENT.into()) },
        DesktopRequest::Read | DesktopRequest::Reconnect => Operation::Check { recipient: RECIPIENT.into(), peek: false },
        DesktopRequest::Ack { id } => Operation::Ack { recipient: RECIPIENT.into(), id },
        DesktopRequest::Status => Operation::StatusBar { action: Action::Snapshot },
        DesktopRequest::Visibility { key, visible } => Operation::StatusBar { action: Action::Visibility { key, visible } },
        DesktopRequest::Display { mode } => Operation::StatusBar { action: Action::Display { mode } },
        DesktopRequest::Refresh => return Err("Usage refresh unavailable on this host".into()),
    };
    let mut result = runtime.call(operation).map_err(|error| error.to_string())?.result;
    if let Some(object) = result.as_object_mut() {
        if object.contains_key("snapshot") {
            object.insert("refresh_providers".into(), json!([]));
        }
    }
    Ok(result)
}
#[tauri::command]
fn desktop_request(window: tauri::WebviewWindow, state: tauri::State<'_, Runtime>, request: DesktopRequest) -> Result<Value, String> {
    if window.label() != "main" || !local_url(&window.url().map_err(|error| error.to_string())?) {
        return Err("Untrusted desktop caller".into());
    }
    dispatch(&state, request)
}
fn local_url(url: &tauri::Url) -> bool {
    url.scheme() == "tauri" && url.host_str() == Some("localhost")
}
#[cfg(feature = "native-smoke")]
#[tauri::command]
fn smoke_complete(app: tauri::AppHandle, success: bool, detail: String) {
    std::fs::write("smoke-result.json", json!({"success":success,"detail":detail}).to_string()).expect("smoke report");
    app.exit(if success { 0 } else { 1 });
}
fn main() {
    let builder = tauri::Builder::default();
    #[cfg(feature = "native-smoke")]
    let builder = builder.invoke_handler(tauri::generate_handler![desktop_request, smoke_complete]);
    #[cfg(not(feature = "native-smoke"))]
    let builder = builder.invoke_handler(tauri::generate_handler![desktop_request]);
    builder.setup(|app| {
        use std::os::unix::fs::PermissionsExt;
        let directory = app.path().app_data_dir()?.join("mailbox");
        std::fs::create_dir_all(&directory)?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        let mailbox = Mailbox::open(directory)?;
        mailbox.register(RECIPIENT)?;
        app.manage(Runtime(mailbox));
        let window = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
            .title("Orquesta").inner_size(1280.0, 900.0)
            .on_navigation(local_url)
            .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny);
        #[cfg(feature = "native-smoke")]
        let window = window.on_page_load(|window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                window.eval(include_str!("../smoke.js")).expect("smoke injection");
            }
        });
        window.build()?;
        Ok(())
    }).run(tauri::generate_context!()).expect("Orquesta desktop startup failed");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_webview_authority_expansion() {
        for input in [r#"{"kind":"send"}"#, r#"{"kind":"inbox","recipient":"session:other"}"#, r#"{"kind":"status","path":"/tmp"}"#, r#"{"kind":"finish","windows":[]}"#] {
            assert!(serde_json::from_str::<DesktopRequest>(input).is_err());
        }
        assert!(!local_url(&"https://example.com".parse().unwrap()));
        assert!(local_url(&"tauri://localhost/index.html".parse().unwrap()));
    }
}
