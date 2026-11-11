use crate::provider_config::{DesktopProviderConfigInput, DesktopProviderConfigSummary};
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
        .provider_store
        .load_provider_config_summary()
        .map_err(|error| error.to_string())?;
    Ok(DesktopRuntimeStatus { daemon, provider })
}

#[tauri::command]
pub async fn save_provider_config(
    input: DesktopProviderConfigInput,
    state: State<'_, AppState>,
) -> Result<DesktopProviderConfigSummary, String> {
    state
        .provider_store
        .save_provider_config(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_provider_config_summary(
    state: State<'_, AppState>,
) -> Result<DesktopProviderConfigSummary, String> {
    state
        .provider_store
        .load_provider_config_summary()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn clear_provider_config(
    state: State<'_, AppState>,
) -> Result<DesktopProviderConfigSummary, String> {
    state
        .provider_store
        .clear_provider_config()
        .map_err(|error| error.to_string())
}
