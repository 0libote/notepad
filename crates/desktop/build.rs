fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "load_draft",
            "load_sync_base",
            "save_draft",
            "load_server",
            "save_server",
            "connection",
            "auth_state",
            "login",
            "setup_owner",
            "logout",
        ]),
    ))
    .expect("Failed to build desktop assets and permissions");
}
