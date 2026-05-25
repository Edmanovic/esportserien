//! ESPASS desktop application library entry point.

mod commands;
mod state;

pub use state::AppState;

/// Entry point for the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::unlock_vault,
            commands::lock_vault,
            commands::get_session_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
