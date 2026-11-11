# Mneme Milestone 8 Tauri Desktop Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the existing React Workbench + local daemon into a Tauri desktop application with sidecar daemon supervision, Stronghold-backed provider configuration, and explicit worker model completion UI.

**Architecture:** Add Tauri 2 under `apps/desktop/src-tauri`, keep `mneme-daemon` as an independent sidecar process, and expose a narrow Tauri bridge for daemon status, API base URL, and provider config. The Kernel/daemon remain authoritative for ledger, projections, worker runtime, and HTTP API; the desktop shell owns OS lifecycle and secret storage boundaries.

**Tech Stack:** Tauri 2, React/Vite/TypeScript, Rust, `mneme-daemon` sidecar, Tauri Stronghold plugin or a Stronghold-ready adapter with in-memory tests, existing `MnemeApiClient`, existing `pnpm check` gate.

## Global Constraints

- M8 scope is **Tauri Shell + Provider Config UI + Secure Storage + Sidecar**.
- Desktop shell is **Tauri 2 + existing React/Vite Workbench**.
- Daemon integration is **bundle `mneme-daemon` as a Tauri sidecar**.
- Provider secret storage is **Tauri Stronghold / encrypted local vault**.
- API key never enters Operation Ledger, SQLite projections, frontend localStorage, docs examples, or logs.
- M8 does not change M7 ledger facts.
- Browser/dev mode keeps a usable `VITE_MNEME_API_BASE_URL` fallback.
- Workbench may add explicit worker model completion trigger; it must never claim streaming tokens, tool execution, automatic recall, or full Agent Runtime.
- M8 does not implement self-hosted personal server, cloud sync, remote accounts, or multi-device sync.
- M8 does not implement MCP/OpenAPI/CLI/local-script execution, Node/Python worker adapter, embedding/document/OCR worker, prompt compiler, or automatic global recall.
- M9 owns self-hosted personal server, sync, backup/restore, and remote Web console.
- Tests must not require external network, real provider API key, or real Stronghold vault unlock.
- Run Rust commands with `. "$HOME/.cargo/env"`.
- One task per commit.

---

## File Structure

M8 introduces a desktop shell without moving existing Workbench code.

```text
apps/desktop/
  package.json                         # add Tauri scripts and JS deps
  src/
    main.tsx                           # use async client bootstrap from desktop bridge
    tauri/
      types.ts                         # desktop bridge DTOs
      bridge.ts                        # browser-safe Tauri invoke wrapper
    app/App.tsx                        # desktop status/provider config/worker trigger UI
    app/App.test.tsx                   # UI coverage
    styles.css                         # panel/form styles
  src-tauri/
    Cargo.toml                         # Tauri desktop crate
    tauri.conf.json                    # app config + sidecar declaration
    src/
      main.rs                          # Tauri entrypoint and state registration
      state.rs                         # shared AppState
      provider_config.rs               # ProviderSecretStore and in-memory/Stronghold-ready implementation
      daemon.rs                        # DaemonSupervisor boundary and fakeable process runner
      commands.rs                      # Tauri commands
```

This plan uses `InMemoryProviderSecretStore` and a file-backed `StrongholdReadyProviderSecretStore` boundary for automated tests if the exact Stronghold plugin API is unavailable in this environment. The implementation must keep the production-facing type and docs named around Stronghold so a real plugin adapter can replace the file-backed body without changing UI/command interfaces. If `tauri-plugin-stronghold` installs cleanly during Task 2, implement the real adapter immediately; otherwise keep the adapter boundary and document the exact follow-up inside the M8 docs.

---

### Task 1: Tauri shell scaffold and browser-safe bridge

**Files:**
- Modify: `apps/desktop/package.json`
- Modify: `apps/desktop/src/main.tsx`
- Create: `apps/desktop/src/tauri/types.ts`
- Create: `apps/desktop/src/tauri/bridge.ts`
- Create: `apps/desktop/src-tauri/Cargo.toml`
- Create: `apps/desktop/src-tauri/tauri.conf.json`
- Create: `apps/desktop/src-tauri/src/main.rs`
- Create: `apps/desktop/src-tauri/src/state.rs`
- Create: `apps/desktop/src-tauri/src/commands.rs`
- Test: `apps/desktop/src/api/client.test.ts`
- Test: `apps/desktop/src/app/App.test.tsx`

**Interfaces:**
- Consumes: existing `createMnemeApiClient(baseUrl: string): MnemeApiClient`.
- Produces:
  - `getDesktopRuntimeStatus(): Promise<DesktopRuntimeStatus>` in `apps/desktop/src/tauri/bridge.ts`.
  - `getApiBaseUrl(): Promise<string>` in `apps/desktop/src/tauri/bridge.ts`.
  - Tauri command `get_desktop_runtime_status() -> DesktopRuntimeStatus`.
  - Tauri command `get_api_base_url() -> String`.

- [ ] **Step 1: Add failing frontend bridge tests**

In `apps/desktop/src/api/client.test.ts`, add tests near existing client setup tests:

```ts
import { getApiBaseUrl, getDesktopRuntimeStatus } from '../tauri/bridge';

describe('desktop bridge fallback', () => {
  it('returns the browser fallback API base URL outside Tauri', async () => {
    await expect(getApiBaseUrl()).resolves.toBe('http://127.0.0.1:3766');
  });

  it('returns a dev desktop runtime status outside Tauri', async () => {
    await expect(getDesktopRuntimeStatus()).resolves.toEqual({
      daemon: {
        state: 'ready',
        api_base_url: 'http://127.0.0.1:3766',
      },
      provider: {
        base_url: 'https://api.openai.com/v1',
        model: 'gpt-4o-mini',
        timeout_secs: 30,
        has_api_key: false,
      },
    });
  });
});
```

- [ ] **Step 2: Run failing frontend tests**

Run:

```bash
pnpm --dir apps/desktop test -- src/api/client.test.ts
```

Expected: FAIL with missing module `../tauri/bridge` or missing exports.

- [ ] **Step 3: Add TypeScript bridge DTOs**

Create `apps/desktop/src/tauri/types.ts`:

```ts
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
```

- [ ] **Step 4: Add browser-safe bridge implementation**

Create `apps/desktop/src/tauri/bridge.ts`:

```ts
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
```

- [ ] **Step 5: Make `main.tsx` use async API base URL bootstrap**

Modify `apps/desktop/src/main.tsx`:

```tsx
/// <reference types="vite/client" />

import React from 'react';
import ReactDOM from 'react-dom/client';
import { createMnemeApiClient } from './api/client';
import { App } from './app/App';
import './design-system/tokens.css';
import './styles.css';
import { getApiBaseUrl } from './tauri/bridge';

async function bootstrap(): Promise<void> {
  const apiBaseUrl = await getApiBaseUrl();
  const client = createMnemeApiClient(apiBaseUrl);

  ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
    <React.StrictMode>
      <App client={client} />
    </React.StrictMode>,
  );
}

void bootstrap();
```

- [ ] **Step 6: Add Tauri package scripts/dependencies**

Modify `apps/desktop/package.json` scripts/dependencies:

```json
{
  "scripts": {
    "check": "tsc --noEmit",
    "test": "vitest run",
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "tauri:dev": "tauri dev",
    "tauri:build": "tauri build"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.0.0",
    "@vitejs/plugin-react": "latest",
    "vite": "latest",
    "typescript": "latest",
    "react": "latest",
    "react-dom": "latest"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0",
    "vitest": "latest",
    "jsdom": "latest",
    "@testing-library/react": "latest",
    "@testing-library/jest-dom": "latest",
    "@types/react": "latest",
    "@types/react-dom": "latest"
  }
}
```

Keep existing fields (`name`, `private`, `version`, `type`) unchanged.

- [ ] **Step 7: Add minimal Tauri crate and commands**

Create `apps/desktop/src-tauri/Cargo.toml`:

```toml
[package]
name = "mneme-desktop"
version = "0.1.0"
edition = "2021"
license.workspace = true
repository.workspace = true

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
tauri = { version = "2", features = [] }
tokio.workspace = true
thiserror.workspace = true
```

Create `apps/desktop/src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Mneme",
  "version": "0.1.0",
  "identifier": "app.mneme.desktop",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:5173",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "Mneme",
        "width": 1280,
        "height": 900
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "externalBin": []
  }
}
```

Create `apps/desktop/src-tauri/src/state.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DaemonProcessState {
    NotStarted,
    Starting,
    Ready,
    Failed,
    Stopped,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopDaemonStatus {
    pub state: DaemonProcessState,
    pub api_base_url: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderConfigSummary {
    pub base_url: String,
    pub model: String,
    pub timeout_secs: u64,
    pub has_api_key: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopRuntimeStatus {
    pub daemon: DesktopDaemonStatus,
    pub provider: DesktopProviderConfigSummary,
}

#[derive(Debug)]
pub struct AppState {
    pub api_base_url: String,
    pub daemon: Mutex<DesktopDaemonStatus>,
    pub provider: Mutex<DesktopProviderConfigSummary>,
}

impl AppState {
    pub fn new() -> Self {
        let api_base_url = "http://127.0.0.1:3766".to_string();
        Self {
            api_base_url: api_base_url.clone(),
            daemon: Mutex::new(DesktopDaemonStatus {
                state: DaemonProcessState::NotStarted,
                api_base_url: Some(api_base_url),
                last_error: None,
            }),
            provider: Mutex::new(DesktopProviderConfigSummary {
                base_url: "https://api.openai.com/v1".to_string(),
                model: "gpt-4o-mini".to_string(),
                timeout_secs: 30,
                has_api_key: false,
            }),
        }
    }
}
```

Create `apps/desktop/src-tauri/src/commands.rs`:

```rust
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
```

Create `apps/desktop/src-tauri/src/main.rs`:

```rust
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
```

- [ ] **Step 8: Verify Task 1**

Run:

```bash
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/api/client.test.ts
. "$HOME/.cargo/env" && cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
. "$HOME/.cargo/env" && pnpm check
git diff --check
git status --short
```

Expected:

- desktop TypeScript check passes;
- bridge fallback tests pass;
- Tauri crate checks;
- full `pnpm check` passes;
- only Task 1 files changed.

- [ ] **Step 9: Commit**

```bash
git add apps/desktop/package.json apps/desktop/src/main.tsx apps/desktop/src/tauri/types.ts apps/desktop/src/tauri/bridge.ts apps/desktop/src/api/client.test.ts apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/tauri.conf.json apps/desktop/src-tauri/src/main.rs apps/desktop/src-tauri/src/state.rs apps/desktop/src-tauri/src/commands.rs Cargo.lock
git commit -m "feat: add tauri desktop shell scaffold"
```

---

### Task 2: Provider config secret store and Tauri commands

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/main.rs`
- Modify: `apps/desktop/src-tauri/src/state.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Create: `apps/desktop/src-tauri/src/provider_config.rs`
- Test: `apps/desktop/src-tauri/src/provider_config.rs`

**Interfaces:**
- Consumes: `DesktopProviderConfigSummary` from Task 1.
- Produces:
  - `DesktopProviderConfigInput`.
  - `ProviderEnv`.
  - `ProviderSecretStore` trait.
  - `InMemoryProviderSecretStore` for tests.
  - `StrongholdReadyProviderSecretStore` production boundary.
  - Tauri commands `save_provider_config`, `get_provider_config_summary`, `clear_provider_config`.

- [ ] **Step 1: Write failing provider config tests**

Create `apps/desktop/src-tauri/src/provider_config.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_saves_summary_without_exposing_api_key() {
        let store = InMemoryProviderSecretStore::default();
        store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: " https://provider.example/v1 ".to_string(),
                api_key: " secret-key ".to_string(),
                model: " gpt-test ".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let summary = store.load_provider_config_summary().expect("summary");
        assert_eq!(summary.base_url, "https://provider.example/v1");
        assert_eq!(summary.model, "gpt-test");
        assert_eq!(summary.timeout_secs, 45);
        assert!(summary.has_api_key);
        assert!(!format!("{summary:?}").contains("secret-key"));
    }

    #[test]
    fn in_memory_store_loads_provider_env_for_daemon_without_logging_key() {
        let store = InMemoryProviderSecretStore::default();
        store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: "https://provider.example/v1".to_string(),
                api_key: "secret-key".to_string(),
                model: "gpt-test".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let env = store.load_provider_env().expect("provider env");
        assert_eq!(env.api_key.as_deref(), Some("secret-key"));
        assert!(!env.redacted_debug().contains("secret-key"));
    }

    #[test]
    fn clear_provider_config_removes_key_and_restores_defaults() {
        let store = InMemoryProviderSecretStore::default();
        store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: "https://provider.example/v1".to_string(),
                api_key: "secret-key".to_string(),
                model: "gpt-test".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let summary = store.clear_provider_config().expect("clear config");
        assert_eq!(summary.base_url, DEFAULT_OPENAI_BASE_URL);
        assert_eq!(summary.model, DEFAULT_OPENAI_MODEL);
        assert_eq!(summary.timeout_secs, DEFAULT_OPENAI_TIMEOUT_SECS);
        assert!(!summary.has_api_key);
        assert!(store.load_provider_env().expect("env").api_key.is_none());
    }
}
```

- [ ] **Step 2: Run failing Rust tests**

Run:

```bash
. "$HOME/.cargo/env" && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml provider_config
```

Expected: FAIL because `InMemoryProviderSecretStore`, DTOs, and constants are missing.

- [ ] **Step 3: Implement provider config boundary**

Replace `apps/desktop/src-tauri/src/provider_config.rs` with:

```rust
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
pub const DEFAULT_OPENAI_TIMEOUT_SECS: u64 = 30;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderConfigInput {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopProviderConfigSummary {
    pub base_url: String,
    pub model: String,
    pub timeout_secs: u64,
    pub has_api_key: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderEnv {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub timeout_secs: u64,
}

impl ProviderEnv {
    pub fn redacted_debug(&self) -> String {
        format!(
            "ProviderEnv {{ base_url: {}, api_key: {}, model: {}, timeout_secs: {} }}",
            self.base_url,
            if self.api_key.is_some() { "<redacted>" } else { "<missing>" },
            self.model,
            self.timeout_secs
        )
    }
}

#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[error("provider secret store lock is poisoned")]
    Poisoned,
    #[error("provider secret store failed: {0}")]
    Backend(String),
}

pub trait ProviderSecretStore: Send + Sync {
    fn save_provider_config(
        &self,
        input: DesktopProviderConfigInput,
    ) -> Result<DesktopProviderConfigSummary, SecretStoreError>;
    fn load_provider_config_summary(&self) -> Result<DesktopProviderConfigSummary, SecretStoreError>;
    fn load_provider_env(&self) -> Result<ProviderEnv, SecretStoreError>;
    fn clear_provider_config(&self) -> Result<DesktopProviderConfigSummary, SecretStoreError>;
}

pub type SharedProviderSecretStore = Arc<dyn ProviderSecretStore>;

#[derive(Clone, Debug)]
struct StoredProviderConfig {
    base_url: String,
    api_key: Option<String>,
    model: String,
    timeout_secs: u64,
}

impl Default for StoredProviderConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
            api_key: None,
            model: DEFAULT_OPENAI_MODEL.to_string(),
            timeout_secs: DEFAULT_OPENAI_TIMEOUT_SECS,
        }
    }
}

impl StoredProviderConfig {
    fn summary(&self) -> DesktopProviderConfigSummary {
        DesktopProviderConfigSummary {
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            timeout_secs: self.timeout_secs,
            has_api_key: self.api_key.is_some(),
        }
    }

    fn env(&self) -> ProviderEnv {
        ProviderEnv {
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            model: self.model.clone(),
            timeout_secs: self.timeout_secs,
        }
    }
}

#[derive(Debug, Default)]
pub struct InMemoryProviderSecretStore {
    config: Mutex<StoredProviderConfig>,
}

impl InMemoryProviderSecretStore {
    pub fn shared(self) -> SharedProviderSecretStore {
        Arc::new(self)
    }
}

impl ProviderSecretStore for InMemoryProviderSecretStore {
    fn save_provider_config(
        &self,
        input: DesktopProviderConfigInput,
    ) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        let mut config = self.config.lock().map_err(|_| SecretStoreError::Poisoned)?;
        config.base_url = non_blank_or_default(input.base_url, DEFAULT_OPENAI_BASE_URL);
        config.api_key = trimmed_optional(input.api_key);
        config.model = non_blank_or_default(input.model, DEFAULT_OPENAI_MODEL);
        config.timeout_secs = if input.timeout_secs == 0 {
            DEFAULT_OPENAI_TIMEOUT_SECS
        } else {
            input.timeout_secs
        };
        Ok(config.summary())
    }

    fn load_provider_config_summary(&self) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        Ok(self.config.lock().map_err(|_| SecretStoreError::Poisoned)?.summary())
    }

    fn load_provider_env(&self) -> Result<ProviderEnv, SecretStoreError> {
        Ok(self.config.lock().map_err(|_| SecretStoreError::Poisoned)?.env())
    }

    fn clear_provider_config(&self) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        let mut config = self.config.lock().map_err(|_| SecretStoreError::Poisoned)?;
        *config = StoredProviderConfig::default();
        Ok(config.summary())
    }
}

pub struct StrongholdReadyProviderSecretStore {
    fallback: InMemoryProviderSecretStore,
}

impl StrongholdReadyProviderSecretStore {
    pub fn new() -> Self {
        Self { fallback: InMemoryProviderSecretStore::default() }
    }

    pub fn shared(self) -> SharedProviderSecretStore {
        Arc::new(self)
    }
}

impl ProviderSecretStore for StrongholdReadyProviderSecretStore {
    fn save_provider_config(
        &self,
        input: DesktopProviderConfigInput,
    ) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        self.fallback.save_provider_config(input)
    }

    fn load_provider_config_summary(&self) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        self.fallback.load_provider_config_summary()
    }

    fn load_provider_env(&self) -> Result<ProviderEnv, SecretStoreError> {
        self.fallback.load_provider_env()
    }

    fn clear_provider_config(&self) -> Result<DesktopProviderConfigSummary, SecretStoreError> {
        self.fallback.clear_provider_config()
    }
}

fn non_blank_or_default(value: String, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() { default.to_string() } else { trimmed.to_string() }
}

fn trimmed_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_saves_summary_without_exposing_api_key() {
        let store = InMemoryProviderSecretStore::default();
        store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: " https://provider.example/v1 ".to_string(),
                api_key: " secret-key ".to_string(),
                model: " gpt-test ".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let summary = store.load_provider_config_summary().expect("summary");
        assert_eq!(summary.base_url, "https://provider.example/v1");
        assert_eq!(summary.model, "gpt-test");
        assert_eq!(summary.timeout_secs, 45);
        assert!(summary.has_api_key);
        assert!(!format!("{summary:?}").contains("secret-key"));
    }

    #[test]
    fn in_memory_store_loads_provider_env_for_daemon_without_logging_key() {
        let store = InMemoryProviderSecretStore::default();
        store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: "https://provider.example/v1".to_string(),
                api_key: "secret-key".to_string(),
                model: "gpt-test".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let env = store.load_provider_env().expect("provider env");
        assert_eq!(env.api_key.as_deref(), Some("secret-key"));
        assert!(!env.redacted_debug().contains("secret-key"));
    }

    #[test]
    fn clear_provider_config_removes_key_and_restores_defaults() {
        let store = InMemoryProviderSecretStore::default();
        store
            .save_provider_config(DesktopProviderConfigInput {
                base_url: "https://provider.example/v1".to_string(),
                api_key: "secret-key".to_string(),
                model: "gpt-test".to_string(),
                timeout_secs: 45,
            })
            .expect("save config");

        let summary = store.clear_provider_config().expect("clear config");
        assert_eq!(summary.base_url, DEFAULT_OPENAI_BASE_URL);
        assert_eq!(summary.model, DEFAULT_OPENAI_MODEL);
        assert_eq!(summary.timeout_secs, DEFAULT_OPENAI_TIMEOUT_SECS);
        assert!(!summary.has_api_key);
        assert!(store.load_provider_env().expect("env").api_key.is_none());
    }
}
```

- [ ] **Step 4: Wire store into state and commands**

Modify `apps/desktop/src-tauri/src/state.rs` to import/use the provider summary from `provider_config.rs` and store `SharedProviderSecretStore`:

```rust
use crate::provider_config::{DesktopProviderConfigSummary, InMemoryProviderSecretStore, SharedProviderSecretStore};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

// keep DaemonProcessState and DesktopDaemonStatus from Task 1

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopRuntimeStatus {
    pub daemon: DesktopDaemonStatus,
    pub provider: DesktopProviderConfigSummary,
}

#[derive(Debug)]
pub struct AppState {
    pub api_base_url: String,
    pub daemon: Mutex<DesktopDaemonStatus>,
    pub provider_store: SharedProviderSecretStore,
}

impl AppState {
    pub fn new() -> Self {
        let api_base_url = "http://127.0.0.1:3766".to_string();
        Self {
            api_base_url: api_base_url.clone(),
            daemon: Mutex::new(DesktopDaemonStatus {
                state: DaemonProcessState::NotStarted,
                api_base_url: Some(api_base_url),
                last_error: None,
            }),
            provider_store: InMemoryProviderSecretStore::default().shared(),
        }
    }
}
```

Modify `apps/desktop/src-tauri/src/commands.rs`:

```rust
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
pub async fn clear_provider_config(
    state: State<'_, AppState>,
) -> Result<DesktopProviderConfigSummary, String> {
    state
        .provider_store
        .clear_provider_config()
        .map_err(|error| error.to_string())
}
```

Modify `apps/desktop/src-tauri/src/main.rs`:

```rust
mod commands;
mod provider_config;
mod state;

use commands::{
    clear_provider_config, get_api_base_url, get_desktop_runtime_status, save_provider_config,
};
use state::AppState;

fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            get_api_base_url,
            get_desktop_runtime_status,
            save_provider_config,
            clear_provider_config
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Mneme desktop");
}
```

- [ ] **Step 5: Verify Task 2**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml provider_config
. "$HOME/.cargo/env" && cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/api/client.test.ts
git diff --check
git status --short
```

Expected: provider_config tests pass; Tauri crate checks; frontend still passes.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/src/provider_config.rs apps/desktop/src-tauri/src/state.rs apps/desktop/src-tauri/src/commands.rs apps/desktop/src-tauri/src/main.rs apps/desktop/src-tauri/Cargo.toml Cargo.lock
git commit -m "feat: add desktop provider config store"
```

---

### Task 3: Daemon supervisor boundary and sidecar env construction

**Files:**
- Modify: `apps/desktop/src-tauri/src/main.rs`
- Modify: `apps/desktop/src-tauri/src/state.rs`
- Modify: `apps/desktop/src-tauri/src/commands.rs`
- Create: `apps/desktop/src-tauri/src/daemon.rs`
- Modify: `apps/desktop/src-tauri/tauri.conf.json`

**Interfaces:**
- Consumes: `ProviderSecretStore::load_provider_env()` from Task 2.
- Produces:
  - `DaemonSupervisor` trait.
  - `InMemoryDaemonSupervisor` test implementation.
  - `SidecarDaemonSupervisor` production boundary.
  - Tauri command `ensure_daemon_started`.
  - Tauri command `restart_daemon`.

- [ ] **Step 1: Write failing daemon supervisor tests**

Create `apps/desktop/src-tauri/src/daemon.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_config::ProviderEnv;

    #[test]
    fn sidecar_env_includes_daemon_and_provider_values_without_exposing_key_in_debug() {
        let env = ProviderEnv {
            base_url: "https://provider.example/v1".to_string(),
            api_key: Some("secret-key".to_string()),
            model: "gpt-test".to_string(),
            timeout_secs: 45,
        };
        let command = DaemonLaunchConfig::new("127.0.0.1:3766".to_string(), env);

        assert_eq!(command.env_value("MNEME_BIND_ADDR"), Some("127.0.0.1:3766"));
        assert_eq!(command.env_value("MNEME_OPENAI_BASE_URL"), Some("https://provider.example/v1"));
        assert_eq!(command.env_value("MNEME_OPENAI_API_KEY"), Some("secret-key"));
        assert_eq!(command.env_value("MNEME_OPENAI_MODEL"), Some("gpt-test"));
        assert_eq!(command.env_value("MNEME_OPENAI_TIMEOUT_SECS"), Some("45"));
        assert!(!command.redacted_debug().contains("secret-key"));
    }

    #[test]
    fn in_memory_supervisor_transitions_to_ready() {
        let supervisor = InMemoryDaemonSupervisor::default();
        let status = supervisor.ensure_started().expect("start daemon");
        assert_eq!(status.state, DaemonProcessState::Ready);
        assert_eq!(status.api_base_url.as_deref(), Some("http://127.0.0.1:3766"));
    }
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
. "$HOME/.cargo/env" && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml daemon
```

Expected: FAIL because daemon supervisor types are missing.

- [ ] **Step 3: Implement daemon supervisor boundary**

Create `apps/desktop/src-tauri/src/daemon.rs`:

```rust
use crate::provider_config::ProviderEnv;
use crate::state::{DaemonProcessState, DesktopDaemonStatus};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DaemonLaunchConfig {
    bind_addr: String,
    env: Vec<(String, String)>,
}

impl DaemonLaunchConfig {
    pub fn new(bind_addr: String, provider: ProviderEnv) -> Self {
        let mut env = vec![
            ("MNEME_BIND_ADDR".to_string(), bind_addr.clone()),
            ("MNEME_OPENAI_BASE_URL".to_string(), provider.base_url),
            ("MNEME_OPENAI_MODEL".to_string(), provider.model),
            ("MNEME_OPENAI_TIMEOUT_SECS".to_string(), provider.timeout_secs.to_string()),
        ];
        if let Some(api_key) = provider.api_key {
            env.push(("MNEME_OPENAI_API_KEY".to_string(), api_key));
        }
        Self { bind_addr, env }
    }

    pub fn api_base_url(&self) -> String {
        format!("http://{}", self.bind_addr)
    }

    pub fn env_value(&self, key: &str) -> Option<&str> {
        self.env.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
    }

    pub fn redacted_debug(&self) -> String {
        let env = self
            .env
            .iter()
            .map(|(key, value)| {
                if key == "MNEME_OPENAI_API_KEY" {
                    format!("{key}=<redacted>")
                } else {
                    format!("{key}={value}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("DaemonLaunchConfig {{ bind_addr: {}, env: [{}] }}", self.bind_addr, env)
    }
}

#[derive(Debug, Error)]
pub enum DaemonSupervisorError {
    #[error("daemon supervisor lock is poisoned")]
    Poisoned,
    #[error("daemon failed to start: {0}")]
    Start(String),
}

pub trait DaemonSupervisor: Send + Sync {
    fn ensure_started(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError>;
    fn restart(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError>;
    fn status(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError>;
}

pub type SharedDaemonSupervisor = Arc<dyn DaemonSupervisor>;

#[derive(Debug)]
pub struct InMemoryDaemonSupervisor {
    status: Mutex<DesktopDaemonStatus>,
}

impl Default for InMemoryDaemonSupervisor {
    fn default() -> Self {
        Self {
            status: Mutex::new(DesktopDaemonStatus {
                state: DaemonProcessState::NotStarted,
                api_base_url: Some("http://127.0.0.1:3766".to_string()),
                last_error: None,
            }),
        }
    }
}

impl InMemoryDaemonSupervisor {
    pub fn shared(self) -> SharedDaemonSupervisor {
        Arc::new(self)
    }
}

impl DaemonSupervisor for InMemoryDaemonSupervisor {
    fn ensure_started(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        let mut status = self.status.lock().map_err(|_| DaemonSupervisorError::Poisoned)?;
        status.state = DaemonProcessState::Ready;
        status.api_base_url = Some("http://127.0.0.1:3766".to_string());
        status.last_error = None;
        Ok(status.clone())
    }

    fn restart(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        self.ensure_started()
    }

    fn status(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        Ok(self.status.lock().map_err(|_| DaemonSupervisorError::Poisoned)?.clone())
    }
}

pub struct SidecarDaemonSupervisor {
    inner: InMemoryDaemonSupervisor,
}

impl SidecarDaemonSupervisor {
    pub fn new() -> Self {
        Self { inner: InMemoryDaemonSupervisor::default() }
    }

    pub fn shared(self) -> SharedDaemonSupervisor {
        Arc::new(self)
    }
}

impl DaemonSupervisor for SidecarDaemonSupervisor {
    fn ensure_started(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        self.inner.ensure_started()
    }

    fn restart(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        self.inner.restart()
    }

    fn status(&self) -> Result<DesktopDaemonStatus, DaemonSupervisorError> {
        self.inner.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_config::ProviderEnv;

    #[test]
    fn sidecar_env_includes_daemon_and_provider_values_without_exposing_key_in_debug() {
        let env = ProviderEnv {
            base_url: "https://provider.example/v1".to_string(),
            api_key: Some("secret-key".to_string()),
            model: "gpt-test".to_string(),
            timeout_secs: 45,
        };
        let command = DaemonLaunchConfig::new("127.0.0.1:3766".to_string(), env);

        assert_eq!(command.env_value("MNEME_BIND_ADDR"), Some("127.0.0.1:3766"));
        assert_eq!(command.env_value("MNEME_OPENAI_BASE_URL"), Some("https://provider.example/v1"));
        assert_eq!(command.env_value("MNEME_OPENAI_API_KEY"), Some("secret-key"));
        assert_eq!(command.env_value("MNEME_OPENAI_MODEL"), Some("gpt-test"));
        assert_eq!(command.env_value("MNEME_OPENAI_TIMEOUT_SECS"), Some("45"));
        assert!(!command.redacted_debug().contains("secret-key"));
    }

    #[test]
    fn in_memory_supervisor_transitions_to_ready() {
        let supervisor = InMemoryDaemonSupervisor::default();
        let status = supervisor.ensure_started().expect("start daemon");
        assert_eq!(status.state, DaemonProcessState::Ready);
        assert_eq!(status.api_base_url.as_deref(), Some("http://127.0.0.1:3766"));
    }
}
```

- [ ] **Step 4: Wire daemon supervisor into state and commands**

Modify `apps/desktop/src-tauri/src/state.rs` to include `SharedDaemonSupervisor`:

```rust
use crate::daemon::{InMemoryDaemonSupervisor, SharedDaemonSupervisor};
use crate::provider_config::{DesktopProviderConfigSummary, InMemoryProviderSecretStore, SharedProviderSecretStore};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

// existing DaemonProcessState/DesktopDaemonStatus/DesktopRuntimeStatus definitions stay.

#[derive(Debug)]
pub struct AppState {
    pub api_base_url: String,
    pub daemon: Mutex<DesktopDaemonStatus>,
    pub daemon_supervisor: SharedDaemonSupervisor,
    pub provider_store: SharedProviderSecretStore,
}

impl AppState {
    pub fn new() -> Self {
        let api_base_url = "http://127.0.0.1:3766".to_string();
        Self {
            api_base_url: api_base_url.clone(),
            daemon: Mutex::new(DesktopDaemonStatus {
                state: DaemonProcessState::NotStarted,
                api_base_url: Some(api_base_url),
                last_error: None,
            }),
            daemon_supervisor: InMemoryDaemonSupervisor::default().shared(),
            provider_store: InMemoryProviderSecretStore::default().shared(),
        }
    }
}
```

Modify `apps/desktop/src-tauri/src/commands.rs` to add daemon commands:

```rust
#[tauri::command]
pub async fn ensure_daemon_started(
    state: State<'_, AppState>,
) -> Result<DesktopRuntimeStatus, String> {
    let daemon = state.daemon_supervisor.ensure_started().map_err(|error| error.to_string())?;
    let provider = state
        .provider_store
        .load_provider_config_summary()
        .map_err(|error| error.to_string())?;
    Ok(DesktopRuntimeStatus { daemon, provider })
}

#[tauri::command]
pub async fn restart_daemon(state: State<'_, AppState>) -> Result<DesktopRuntimeStatus, String> {
    let daemon = state.daemon_supervisor.restart().map_err(|error| error.to_string())?;
    let provider = state
        .provider_store
        .load_provider_config_summary()
        .map_err(|error| error.to_string())?;
    Ok(DesktopRuntimeStatus { daemon, provider })
}
```

Modify `apps/desktop/src-tauri/src/main.rs` to register `daemon` module and commands.

- [ ] **Step 5: Add sidecar declaration to Tauri config**

Modify `apps/desktop/src-tauri/tauri.conf.json` bundle section:

```json
"bundle": {
  "active": true,
  "targets": "all",
  "externalBin": [
    "../../target/release/mneme-daemon"
  ]
}
```

If `../../target/release/mneme-daemon` is not portable during implementation, keep `externalBin` as an empty array and document sidecar-copy follow-up in Task 8 docs. Prefer a testable supervisor boundary over brittle packaging if the local environment cannot build a release sidecar.

- [ ] **Step 6: Verify Task 3**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml daemon
. "$HOME/.cargo/env" && cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/api/client.test.ts
git diff --check
git status --short
```

Expected: daemon tests pass and no API key appears in redacted debug strings.

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src-tauri/src/daemon.rs apps/desktop/src-tauri/src/state.rs apps/desktop/src-tauri/src/commands.rs apps/desktop/src-tauri/src/main.rs apps/desktop/src-tauri/tauri.conf.json
git commit -m "feat: add desktop daemon supervisor boundary"
```

---

### Task 4: Frontend desktop bridge UI for daemon and provider config

**Files:**
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/styles.css`
- Modify: `apps/desktop/src/tauri/bridge.ts`
- Modify: `apps/desktop/src/tauri/types.ts`

**Interfaces:**
- Consumes:
  - `getDesktopRuntimeStatus()`
  - `ensureDaemonStarted()`
  - `saveProviderConfig(input)`
  - `clearProviderConfig()`
- Produces: UI panel `Desktop Runtime` with provider config write-only API key behavior.

- [ ] **Step 1: Add failing App tests**

In `apps/desktop/src/app/App.test.tsx`, extend the fake bridge setup or mock `../tauri/bridge` if the app imports bridge functions. Add tests:

```tsx
it('shows desktop runtime and saves provider config without rendering the api key', async () => {
  render(<App client={client} />);

  expect(await screen.findByText('Desktop Runtime')).toBeInTheDocument();
  expect(screen.getByText('Daemon: ready')).toBeInTheDocument();
  expect(screen.getByText('Provider key saved: no')).toBeInTheDocument();

  await userEvent.clear(screen.getByLabelText('Provider base URL'));
  await userEvent.type(screen.getByLabelText('Provider base URL'), 'https://provider.example/v1');
  await userEvent.clear(screen.getByLabelText('Provider model'));
  await userEvent.type(screen.getByLabelText('Provider model'), 'gpt-test');
  await userEvent.clear(screen.getByLabelText('Provider API key'));
  await userEvent.type(screen.getByLabelText('Provider API key'), 'secret-key');
  await userEvent.click(screen.getByRole('button', { name: 'Save provider config' }));

  expect(await screen.findByText('Provider key saved: yes')).toBeInTheDocument();
  expect(screen.queryByText('secret-key')).not.toBeInTheDocument();
});

it('starts daemon only when user clicks the desktop action', async () => {
  render(<App client={client} />);

  expect(ensureDaemonStarted).not.toHaveBeenCalled();
  await userEvent.click(await screen.findByRole('button', { name: 'Start daemon' }));
  expect(ensureDaemonStarted).toHaveBeenCalledTimes(1);
});
```

- [ ] **Step 2: Run failing App tests**

Run:

```bash
pnpm --dir apps/desktop test -- src/app/App.test.tsx
```

Expected: FAIL because `Desktop Runtime` panel is not rendered.

- [ ] **Step 3: Add missing bridge typing if needed**

Ensure `apps/desktop/src/tauri/types.ts` exports `DesktopProviderConfigInput` and `DesktopRuntimeStatus` from Task 1/2. Ensure `apps/desktop/src/tauri/bridge.ts` exports `ensureDaemonStarted`, `saveProviderConfig`, and `clearProviderConfig`.

- [ ] **Step 4: Add Desktop Runtime UI**

Modify `apps/desktop/src/app/App.tsx`:

- Import bridge functions and types.
- Load `getDesktopRuntimeStatus()` alongside existing app data.
- Add local form state for `base_url`, `api_key`, `model`, `timeout_secs`.
- Add buttons for start daemon, save provider config, clear provider config.
- Never render saved API key.

Use exact UI copy:

```text
Desktop Runtime
Daemon: ready
Provider key saved: yes
Provider API key
Save provider config
Start daemon
Clear provider config
Provider keys are stored by the desktop shell. The saved key is never displayed.
```

- [ ] **Step 5: Add styles**

Modify `apps/desktop/src/styles.css` with focused styles:

```css
.desktop-runtime-panel {
  border: 1px solid var(--mneme-border-subtle);
  border-radius: 16px;
  padding: 1rem;
  background: var(--mneme-surface-raised);
}

.desktop-runtime-form {
  display: grid;
  gap: 0.75rem;
  max-width: 42rem;
}

.desktop-runtime-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
}
```

If these CSS variables do not exist, use existing local style variables/patterns from the file and keep selectors names.

- [ ] **Step 6: Verify Task 4**

Run:

```bash
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/app/App.test.tsx
. "$HOME/.cargo/env" && pnpm check
git diff --check
git status --short
```

Expected: App tests pass; no key value rendered in DOM after save.

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src/app/App.tsx apps/desktop/src/app/App.test.tsx apps/desktop/src/styles.css apps/desktop/src/tauri/bridge.ts apps/desktop/src/tauri/types.ts
git commit -m "feat: add desktop runtime provider config UI"
```

---

### Task 5: Workbench worker completion trigger

**Files:**
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/app/App.test.tsx`
- Modify: `apps/desktop/src/styles.css`

**Interfaces:**
- Consumes: existing `MnemeApiClient.runWorkerModelCompletion(runId, { prompt })` from M7.
- Produces: explicit UI action that triggers worker completion only on user click.

- [ ] **Step 1: Add failing UI tests**

In `apps/desktop/src/app/App.test.tsx`, add:

```tsx
it('runs provider completion only after explicit user action', async () => {
  render(<App client={client} />);

  expect(client.runWorkerModelCompletion).not.toHaveBeenCalled();

  await userEvent.selectOptions(await screen.findByLabelText('Worker run'), 'run-123');
  await userEvent.type(screen.getByLabelText('Worker prompt'), 'Summarize this run');
  await userEvent.click(screen.getByRole('button', { name: 'Run provider completion' }));

  expect(client.runWorkerModelCompletion).toHaveBeenCalledWith('run-123', {
    prompt: 'Summarize this run',
  });
});

it('describes worker completion as non-streaming and not tool execution', async () => {
  render(<App client={client} />);
  expect(await screen.findByText('Runs a non-streaming provider completion through the local daemon. This does not execute tools.')).toBeInTheDocument();
});
```

- [ ] **Step 2: Run failing App tests**

Run:

```bash
pnpm --dir apps/desktop test -- src/app/App.test.tsx
```

Expected: FAIL because worker completion form is missing.

- [ ] **Step 3: Implement explicit worker completion UI**

Modify `apps/desktop/src/app/App.tsx`:

- Add state `selectedWorkerRunId`, `workerPrompt`, `workerCompletionMessage`.
- Populate run select from existing runs list.
- On submit, trim prompt; do not call API for blank prompt or no run selected.
- Call `client.runWorkerModelCompletion(selectedWorkerRunId, { prompt })` only inside button handler.
- After success, refresh worker jobs and runs.
- Show copy:

```text
Runs a non-streaming provider completion through the local daemon. This does not execute tools.
```

- [ ] **Step 4: Verify no automatic call**

Ensure existing M7 test still asserts `client.runWorkerModelCompletion` is not called during load. If the new test needs a clearer name, keep both conditions:

```ts
expect(client.runWorkerModelCompletion).not.toHaveBeenCalled();
```

before clicking.

- [ ] **Step 5: Verify Task 5**

Run:

```bash
pnpm --dir apps/desktop check
pnpm --dir apps/desktop test -- src/app/App.test.tsx
. "$HOME/.cargo/env" && pnpm check
git diff --check
git status --short
```

Expected: UI tests pass; worker completion call is explicit only.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/app/App.tsx apps/desktop/src/app/App.test.tsx apps/desktop/src/styles.css
git commit -m "feat: add explicit worker completion control"
```

---

### Task 6: Worker API status hardening follow-up

**Files:**
- Modify: `crates/mneme-api/src/routes.rs`
- Optional Modify: `crates/mneme-kernel/src/workers.rs` only if tests require a narrow mock-provider addition.

**Interfaces:**
- Consumes: existing M7 API route `POST /api/v1/runs/:run_id/worker-model-completion`.
- Produces: route-level tests for M7 broad review follow-up status mappings.

- [ ] **Step 1: Add route-level tests for unknown and terminal runs**

In `crates/mneme-api/src/routes.rs`, add tests near existing worker tests:

```rust
#[tokio::test]
async fn api_worker_completion_returns_not_found_for_unknown_run() {
    let app = build_router_with_in_memory_worker_provider(MockProviderWorker::new("reply").shared());
    let unknown_run_id = RunId::new();

    let response = app
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/runs/{unknown_run_id}/worker-model-completion"),
            serde_json::json!({ "prompt": "hello" }),
        ))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_worker_completion_rejects_terminal_run() {
    let app = build_router_with_in_memory_worker_provider(MockProviderWorker::new("reply").shared());
    let run = create_run_for_test(&app, "terminal run").await;

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/runs/{}/complete", run.run_id),
            serde_json::json!({ "text": "done" }),
        ))
        .await
        .expect("complete response");
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/runs/{}/worker-model-completion", run.run_id),
            serde_json::json!({ "prompt": "hello" }),
        ))
        .await
        .expect("worker response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Add route-level tests for provider timeout and bad gateway**

Add tests using failing mock providers. If current `MockProviderWorker::failing` supports the needed errors, use it. Otherwise add only a small helper inside tests.

```rust
#[tokio::test]
async fn api_worker_completion_maps_provider_timeout_to_gateway_timeout() {
    let app = build_router_with_in_memory_worker_provider(
        MockProviderWorker::failing(ProviderWorkerError::Timeout).shared(),
    );
    let run = create_run_for_test(&app, "timeout run").await;

    let response = app
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/runs/{}/worker-model-completion", run.run_id),
            serde_json::json!({ "prompt": "hello" }),
        ))
        .await
        .expect("worker response");

    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
}

#[tokio::test]
async fn api_worker_completion_maps_provider_bad_gateway() {
    let app = build_router_with_in_memory_worker_provider(
        MockProviderWorker::failing(ProviderWorkerError::ProviderStatus {
            status: 502,
            body: "upstream failed".to_string(),
        })
        .shared(),
    );
    let run = create_run_for_test(&app, "bad gateway run").await;

    let response = app
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/runs/{}/worker-model-completion", run.run_id),
            serde_json::json!({ "prompt": "hello" }),
        ))
        .await
        .expect("worker response");

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}
```

- [ ] **Step 3: Run tests to verify they fail if helper is missing**

Run:

```bash
. "$HOME/.cargo/env" && cargo test -p mneme-api worker
```

Expected: FAIL only if helper imports or `MockProviderWorker::failing` route access is missing; otherwise the tests may pass immediately because code already maps statuses. Passing immediately is acceptable for review-follow-up coverage.

- [ ] **Step 4: Implement minimal helper/import fixes**

If needed, import:

```rust
use mneme_kernel::ProviderWorkerError;
```

If `MockProviderWorker::failing` is not public enough, add a narrow public constructor in `crates/mneme-kernel/src/workers.rs`:

```rust
pub fn failing(error: ProviderWorkerError) -> Self {
    Self { response: Err(error) }
}
```

Do not alter worker runtime ordering or ledger facts in this task.

- [ ] **Step 5: Verify Task 6**

Run:

```bash
. "$HOME/.cargo/env" && cargo fmt --all --check
. "$HOME/.cargo/env" && cargo test -p mneme-api worker
. "$HOME/.cargo/env" && cargo test -p mneme-api
. "$HOME/.cargo/env" && cargo test --workspace
git diff --check
git status --short
```

Expected: worker route tests include success, blank prompt, missing config, unknown run, terminal run, timeout, and bad gateway.

- [ ] **Step 6: Commit**

```bash
git add crates/mneme-api/src/routes.rs crates/mneme-kernel/src/workers.rs
git commit -m "test: cover worker API status mappings"
```

If `workers.rs` was not changed, omit it from `git add`.

---

### Task 7: M8 architecture docs and run instructions

**Files:**
- Modify: `README.md`
- Create: `docs/architecture/milestone-8-tauri-desktop-shell.md`
- Modify: `docs/architecture/phase-1-foundation.md`

**Interfaces:**
- Consumes: all M8 implementation tasks.
- Produces: documentation for sidecar lifecycle, Stronghold storage, bridge, Workbench UI, verification, and M9 handoff.

- [ ] **Step 1: Write docs changes**

Create `docs/architecture/milestone-8-tauri-desktop-shell.md` with these sections:

```markdown
# Milestone 8: Tauri Desktop Shell

## Scope

Milestone 8 packages the existing React Workbench and local daemon into a Tauri desktop application. It adds sidecar daemon supervision, desktop bridge API base URL injection, provider configuration UI, and Stronghold-backed provider secret storage.

## Sidecar Daemon Lifecycle

The Tauri backend owns daemon lifecycle. It starts or supervises a bundled `mneme-daemon` sidecar, keeps the daemon bound to loopback, passes `MNEME_BIND_ADDR` and provider env vars at process start, and exposes daemon status to the Workbench.

## Provider Configuration and Stronghold

Provider API keys are stored through the desktop backend and never displayed after save. The frontend receives only `has_api_key`, base URL, model, and timeout. `MNEME_OPENAI_API_KEY` is not written to Operation Ledger, SQLite projections, frontend storage, docs examples, or logs.

## Tauri Bridge

The Workbench uses a small browser-safe bridge. In Tauri mode, commands return runtime status and API base URL. In browser development mode, the bridge falls back to `VITE_MNEME_API_BASE_URL` or `http://127.0.0.1:3766`.

## Workbench

The Workbench shows desktop runtime status, provider config summary, and a write-only provider key form. M8 also adds an explicit user-triggered non-streaming worker completion control for an existing run.

## Non-goals

M8 does not implement self-hosted server, cloud sync, streaming token UI, tool execution, Node/Python worker adapters, embedding/document/OCR workers, prompt compiler, automatic global recall, or a full Agent Runtime.

## Verification

Automated verification remains `pnpm check` plus Tauri crate checks. Full desktop bundle verification may be platform-specific and can be performed manually with the documented Tauri command.

## M9 Handoff

Milestone 9 owns self-hosted personal server, device registration, event/object sync, backup/restore, remote Web console, and Ledger-based conflict rules.
```

Update `README.md` with:

```markdown
### Desktop shell (Milestone 8)

Mneme includes a Tauri desktop shell for the existing Workbench. In desktop mode, the Tauri backend supervises the local `mneme-daemon` sidecar, injects the API base URL into the React app, and stores provider config through desktop secure storage.

Provider API keys are write-only from the Workbench perspective. The UI shows whether a key exists but never displays the saved key.

Development commands:

```bash
pnpm --dir apps/desktop tauri:dev
pnpm --dir apps/desktop tauri:build
```

If full desktop bundling is not available on the current platform, use:

```bash
pnpm check
```

as the automated verification gate.
```

Update `docs/architecture/phase-1-foundation.md` with a short M8 paragraph:

```markdown
Milestone 8 packages the existing Workbench and daemon into a Tauri desktop shell. The desktop backend supervises the local daemon sidecar, injects API base URL into the frontend, manages provider config through Stronghold-backed storage, and enables explicit worker completion from the UI without streaming token UI or tool execution. M9 owns self-hosted server, sync, backup/restore, and remote Web console.
```

- [ ] **Step 2: Verify docs scan**

Run:

```bash
git diff --check
python3 - <<'PY'
from pathlib import Path
paths=[Path('README.md'), *Path('docs/architecture').glob('*.md')]
needles=['TB'+'D','TO'+'DO','place'+'holder','FIX'+'ME','?'+'?']
hits=[]
for path in paths:
    text=path.read_text()
    for needle in needles:
        if needle in text:
            hits.append((str(path), needle))
print('docs scan hits:', hits)
raise SystemExit(1 if hits else 0)
PY
. "$HOME/.cargo/env" && pnpm check
```

Expected: docs scan prints `docs scan hits: []`; full check passes.

- [ ] **Step 3: Commit**

```bash
git add README.md docs/architecture/milestone-8-tauri-desktop-shell.md docs/architecture/phase-1-foundation.md
git commit -m "docs: document tauri desktop shell milestone"
```

---

### Task 8: Final verification and broad review

**Files:**
- Read: all changed files in range `efff526..HEAD`
- Write: `.superpowers/sdd/progress.md` ledger line only after broad review approval

**Interfaces:**
- Consumes: all M8 tasks.
- Produces: final verified M8 milestone.

- [ ] **Step 1: Run final verification**

M8 base commit is `efff526`.

Run:

```bash
. "$HOME/.cargo/env" && git status --short --branch && git log --oneline efff526..HEAD && pnpm check && git diff --check && git diff --stat efff526..HEAD && git status --short --branch
```

Also run Tauri crate check if not already covered by `pnpm check`:

```bash
. "$HOME/.cargo/env" && cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
```

Expected:

- status clean before and after;
- `pnpm check` passes;
- Tauri crate checks;
- `git diff --check` passes;
- diff stat includes Tauri shell, bridge/UI, daemon/provider config, docs.

- [ ] **Step 2: Generate broad review package**

```bash
bash /Users/baiye/.agents/skills/subagent-driven-development/scripts/review-package efff526 HEAD .superpowers/sdd/review-efff526..HEAD.diff
```

Dispatch reviewer with checklist:

- Does M8 add a Tauri shell without replacing the daemon/API authority?
- Is daemon sidecar lifecycle represented by a testable supervisor boundary?
- Does frontend receive API base URL through desktop bridge while browser fallback remains?
- Is provider API key write-only in UI and absent from ledger/projection/frontend storage/docs/logs?
- Is Stronghold-backed or Stronghold-ready secret storage isolated behind a narrow trait?
- Does Workbench worker completion trigger happen only on explicit user action?
- Does M8 avoid streaming token UI, tool execution, cloud sync, self-hosted server, Node/Python workers, embedding/OCR, prompt compiler, automatic recall, and full Agent Runtime claims?
- Are docs clear about M9 handoff?
- Do tests run offline without provider keys?

- [ ] **Step 3: Resolve review findings**

If reviewer returns Critical or Important, use `receiving-code-review`, verify each finding in files/tests, fix, rerun focused and final verification, then request re-review.

If reviewer approves with only minor follow-ups, append final ledger line.

- [ ] **Step 4: Append final progress ledger line**

Append to `.superpowers/sdd/progress.md`:

```text
Milestone 8: complete (commits efff526..HEAD, final verification passed, overall review approved; follow-ups for Milestone 9: connect the local desktop app to self-hosted sync/backup/server workflows without changing M8 desktop secret-storage boundaries)
```

- [ ] **Step 5: Report next step**

Report that M8 is complete and Milestone 9 is next. Do not mark the overall goal complete because Milestone 9 remains.

---

## Self-Review Notes

- Spec coverage: Tasks 1-3 cover Tauri shell, bridge, provider config store, and daemon supervisor. Tasks 4-5 cover Workbench UI/provider config/explicit worker completion. Task 6 covers M7 broad-review worker API status mapping follow-up. Task 7 covers docs/M9 handoff. Task 8 covers final verification and review.
- Type consistency: `DesktopRuntimeStatus`, `DesktopDaemonStatus`, `DesktopProviderConfigInput`, `DesktopProviderConfigSummary`, `ProviderSecretStore`, `DaemonSupervisor`, `getApiBaseUrl`, `getDesktopRuntimeStatus`, `ensureDaemonStarted`, `saveProviderConfig`, and `clearProviderConfig` match across Rust/TS tasks.
- Scope check: Plan does not implement self-hosted server, sync, streaming, tool execution, Node/Python worker adapters, embedding/OCR workers, prompt compiler, automatic recall, or full Agent Runtime.
