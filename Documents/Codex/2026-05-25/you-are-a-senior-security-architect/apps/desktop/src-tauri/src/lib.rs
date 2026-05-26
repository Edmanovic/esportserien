//! ESPASS desktop application library entry point.

mod commands;
mod ipc_server;
mod state;
mod sync;
mod tray;

pub use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(state::AppState::default())
        .setup(|app| {
            // Register this app to start with Windows/macOS login.
            use tauri_plugin_autostart::ManagerExt;
            let _ = app.autolaunch().enable();

            // Start WebSocket IPC server (extension bridge).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = ipc_server::run_ipc_server(handle).await {
                    eprintln!("[espass] IPC server error: {e}");
                }
            });

            // System tray.
            tray::setup_tray(app)?;

            // Auto-lock background timer.
            tray::start_autolock_task(app.handle().clone());

            Ok(())
        })
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
            commands::set_autolock_timeout,
            commands::get_lock_status,
            commands::sync_configure,
            commands::sync_now_cmd,
            commands::get_sync_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
