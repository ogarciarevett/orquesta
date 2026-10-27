// Copyright (c) 2026 Lovecast Inc. Copyright (c) 2026 Omar Garcia. SPDX-License-Identifier: MIT
fn main() {
    #[cfg(feature = "native-smoke")]
    const COMMANDS: &[&str] = &["desktop_request", "smoke_complete"];
    #[cfg(not(feature = "native-smoke"))]
    const COMMANDS: &[&str] = &["desktop_request"];
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("desktop permission manifest");
}
