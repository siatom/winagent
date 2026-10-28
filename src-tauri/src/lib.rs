mod approval;
mod commands;
mod db;
mod secrets;
mod state;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = AppState::init(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::create_session,
            commands::rename_session,
            commands::delete_session,
            commands::set_session_model,
            commands::get_messages,
            commands::save_provider,
            commands::delete_provider,
            commands::set_default_provider,
            commands::fetch_models,
            commands::save_settings,
            commands::send_message,
            commands::cancel_run,
            commands::resolve_approval,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
