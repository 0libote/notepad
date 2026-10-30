#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use notepad_core::{AuthState, Workspace};
use notepad_storage::DesktopClient;
use tauri::Manager;
#[tauri::command]
fn connection(state: tauri::State<'_, DesktopClient>) -> Option<String> {
    state.connection()
}
#[tauri::command]
fn load_draft(state: tauri::State<'_, DesktopClient>) -> Result<Option<Workspace>, String> {
    state.load_draft()
}
#[tauri::command]
fn save_draft(
    state: tauri::State<'_, DesktopClient>,
    workspace: Workspace,
    base: Option<Workspace>,
) -> Result<(), String> {
    state.save_synced_draft(&workspace, base.as_ref())
}
#[tauri::command]
fn load_sync_base(state: tauri::State<'_, DesktopClient>) -> Result<Option<Workspace>, String> {
    state.load_sync_base()
}
#[tauri::command]
async fn load_server(state: tauri::State<'_, DesktopClient>) -> Result<Workspace, String> {
    state.load_server().await
}
#[tauri::command]
async fn save_server(
    state: tauri::State<'_, DesktopClient>,
    workspace: Workspace,
) -> Result<Workspace, String> {
    state.save_server(workspace).await
}
#[tauri::command]
async fn auth_state(state: tauri::State<'_, DesktopClient>) -> Result<AuthState, String> {
    state.auth_state().await
}
#[tauri::command]
async fn login(state: tauri::State<'_, DesktopClient>, password: String) -> Result<(), String> {
    state.login(password).await
}
#[tauri::command]
async fn setup_owner(
    state: tauri::State<'_, DesktopClient>,
    password: String,
) -> Result<(), String> {
    state.setup_owner(password).await
}
#[tauri::command]
async fn logout(state: tauri::State<'_, DesktopClient>) -> Result<(), String> {
    state.logout().await
}
fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let server = std::env::var("NOTEPAD_SERVER_URL").ok();
            let client = DesktopClient::new(directory.join("drafts.db"), server.as_deref())
                .map_err(std::io::Error::other)?;
            app.manage(client);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_draft,
            load_sync_base,
            save_draft,
            load_server,
            save_server,
            connection,
            auth_state,
            login,
            setup_owner,
            logout
        ])
        .run(tauri::generate_context!())
        .expect("Notepad desktop could not start");
}
