import type { DesktopProviderConfigInput, DesktopProviderConfigSummary, DesktopRuntimeStatus } from './types';

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

const DEFAULT_API_BASE_URL = 'http://127.0.0.1:3766';
const DEFAULT_PROVIDER: DesktopProviderConfigSummary = {
  base_url: 'https://api.openai.com/v1',
  model: 'gpt-4o-mini',
  timeout_secs: 30,
  has_api_key: false,
};

async function tauriInvoke(): Promise<Invoke | undefined> {
  const candidate = (globalThis as { __TAURI__?: { core?: { invoke?: Invoke }; invoke?: Invoke } }).__TAURI__;
  return candidate?.core?.invoke || candidate?.invoke;
}

export async function getApiBaseUrl(): Promise<string> {
  const invoke = await tauriInvoke();
  if (!invoke) {
    return import.meta.env.VITE_MNEME_API_BASE_URL || DEFAULT_API_BASE_URL;
  }
  return invoke<string>('get_api_base_url');
}

export async function getDesktopRuntimeStatus(): Promise<DesktopRuntimeStatus> {
  const invoke = await tauriInvoke();
  if (!invoke) {
    const apiBaseUrl = import.meta.env.VITE_MNEME_API_BASE_URL || DEFAULT_API_BASE_URL;
    return {
      daemon: { state: 'ready', api_base_url: apiBaseUrl },
      provider: DEFAULT_PROVIDER,
    };
  }
  return invoke<DesktopRuntimeStatus>('get_desktop_runtime_status');
}

export async function ensureDaemonStarted(): Promise<DesktopRuntimeStatus> {
  const invoke = await tauriInvoke();
  if (!invoke) {
    return getDesktopRuntimeStatus();
  }
  return invoke<DesktopRuntimeStatus>('ensure_daemon_started');
}

export async function saveProviderConfig(input: DesktopProviderConfigInput): Promise<DesktopProviderConfigSummary> {
  const invoke = await tauriInvoke();
  if (!invoke) {
    return {
      base_url: input.base_url,
      model: input.model,
      timeout_secs: input.timeout_secs,
      has_api_key: input.api_key.trim().length > 0,
    };
  }
  return invoke<DesktopProviderConfigSummary>('save_provider_config', { input });
}

export async function clearProviderConfig(): Promise<DesktopProviderConfigSummary> {
  const invoke = await tauriInvoke();
  if (!invoke) {
    return DEFAULT_PROVIDER;
  }
  return invoke<DesktopProviderConfigSummary>('clear_provider_config');
}
