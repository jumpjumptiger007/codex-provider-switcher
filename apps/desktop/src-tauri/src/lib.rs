mod commands;
mod dto;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_providers,
            commands::get_status,
            commands::get_credential_status,
            commands::save_credential,
            commands::get_models,
            commands::switch_model,
            commands::safety::run_doctor,
            commands::safety::restore_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running Codex Provider Switcher");
}
