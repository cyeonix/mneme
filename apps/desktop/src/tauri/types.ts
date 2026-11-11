export type DaemonProcessState = 'not_started' | 'starting' | 'ready' | 'failed' | 'stopped';

export interface DesktopDaemonStatus {
  state: DaemonProcessState;
  api_base_url?: string;
  last_error?: string;
}

export interface DesktopProviderConfigSummary {
  base_url: string;
  model: string;
  timeout_secs: number;
  has_api_key: boolean;
}

export interface DesktopProviderConfigInput {
  base_url: string;
  api_key: string;
  model: string;
  timeout_secs: number;
}

export interface DesktopRuntimeStatus {
  daemon: DesktopDaemonStatus;
  provider: DesktopProviderConfigSummary;
}
