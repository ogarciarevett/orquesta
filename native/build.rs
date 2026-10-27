// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
fn main() {
    let commands: &[&str] = if cfg!(feature = "native-smoke") {
        &["desktop_request", "smoke_complete"]
    } else {
        &["desktop_request"]
    };
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(commands),
        ),
    )
    .expect("desktop permission manifest");
}
