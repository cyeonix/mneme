use crate::state::{AppState, DesktopRuntimeStatus};
use tauri::State;

#[tauri::command]
pub async fn get_api_base_url(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.api_base_url.clone())
}

#[tauri::command]
pub async fn get_desktop_runtime_status(
    state: State<'_, AppState>,
) -> Result<DesktopRuntimeStatus, String> {
    let daemon = state
        .daemon
        .lock()
        .map_err(|_| "desktop daemon state lock poisoned".to_string())?
        .clone();
    let provider = state
        .provider
        .lock()
        .map_err(|_| "desktop provider state lock poisoned".to_string())?
        .clone();
    Ok(DesktopRuntimeStatus { daemon, provider })
}
