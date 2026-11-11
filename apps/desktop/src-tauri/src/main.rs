mod commands;
mod daemon;
mod provider_config;
mod state;

use commands::{
    clear_provider_config, ensure_daemon_started, get_api_base_url, get_desktop_runtime_status,
    get_provider_config_summary, restart_daemon, save_provider_config,
};
use state::AppState;

fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            get_api_base_url,
            get_desktop_runtime_status,
            ensure_daemon_started,
            restart_daemon,
            save_provider_config,
            get_provider_config_summary,
            clear_provider_config
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Mneme desktop");
}
