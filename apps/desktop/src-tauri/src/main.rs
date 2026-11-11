mod commands;
mod state;

use commands::{get_api_base_url, get_desktop_runtime_status};
use state::AppState;

fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            get_api_base_url,
            get_desktop_runtime_status
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Mneme desktop");
}
