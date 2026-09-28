mod commands;
mod dto;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_providers,
            commands::get_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running Codex Provider Switcher");
}
