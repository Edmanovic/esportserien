//! ESPASS desktop application library entry point.

mod commands;
mod state;
mod sync;

pub use state::AppState;

/// Entry point for the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::vault_exists,
            commands::create_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::get_session_status,
            commands::list_credentials,
            commands::add_credential,
            commands::get_credential,
            commands::delete_credential,
            commands::update_credential,
            commands::generate_password,
            commands::import_credentials,
            commands::import_credentials_json,
            commands::export_credentials_csv,
            commands::export_credentials_json,
            commands::sync_configure,
            commands::sync_now_cmd,
            commands::get_sync_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
