// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&["desktop_request", "smoke_complete"]),
        ),
    )
    .expect("desktop permission manifest");
}
