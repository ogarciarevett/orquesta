// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
fn main() {
    let mut commands = vec!["desktop_request"];
    if cfg!(feature = "native-smoke") {
        commands.push("smoke_complete");
    }
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(&commands)),
    )
    .expect("desktop permission manifest");
}
