# Mneme Milestone 8 Tauri Desktop Shell Design

## 1. Decision Summary

Milestone 8 turns the existing daemon + React Workbench into an installable desktop application.

User-approved direction:

> **Tauri Shell + Provider Config UI + Secure Storage + Sidecar**

Specific choices:

- Desktop shell: **Tauri 2 + existing React/Vite Workbench**.
- Daemon integration: **bundle `mneme-daemon` as a Tauri sidecar**.
- Provider secret storage: **Tauri Stronghold / encrypted local vault**.
- Provider config flow: desktop UI stores provider settings through Tauri commands; API key never enters Operation Ledger, SQLite projections, frontend localStorage, docs examples, or logs.
- API connection: the React Workbench receives its API base URL from the Tauri shell when running inside desktop; browser/dev mode can keep `VITE_MNEME_API_BASE_URL` fallback.

M8 does not change M7 ledger facts. It adds desktop packaging, sidecar supervision, provider config management, and a controlled desktop UI path for triggering the already-existing worker model completion API.

## 2. Current Baseline

Current repository state before M8:

- No existing Tauri config (`tauri.conf.*` not present).
- `apps/desktop` is a Vite/React application.
- `apps/desktop/src/main.tsx` currently creates the API client from `VITE_MNEME_API_BASE_URL` with a fallback to `http://127.0.0.1:3766`, and already notes that Tauri can inject this later.
- `mneme-daemon` is an independent Rust binary exposing local HTTP API.
- `mneme-daemon` reads provider config from env vars:
  - `MNEME_OPENAI_BASE_URL`
  - `MNEME_OPENAI_API_KEY`
  - `MNEME_OPENAI_MODEL`
  - `MNEME_OPENAI_TIMEOUT_SECS`
- M7 already implemented:
  - `OpenAiCompatibleProviderWorker`
  - `WorkerRuntimeService`
  - `POST /api/v1/runs/:run_id/worker-model-completion`
  - `GET /api/v1/worker-jobs`
  - read-only Workbench `Worker Runtime` job list
- M7 broad review follow-ups for M8:
  - move provider configuration and worker process lifecycle into Tauri desktop packaging;
  - add route-level tests for remaining worker API status mappings where appropriate;
  - revisit or document `WorkerJobStarted` side-effect semantics for unknown/terminal run attempts.

## 3. Goals

M8 must provide a real desktop application slice:

1. Add a Tauri 2 shell for `apps/desktop`.
2. Bundle and supervise `mneme-daemon` as a Tauri sidecar.
3. Choose a local API port/address and inject the actual API base URL into the React Workbench.
4. Add a desktop provider configuration flow backed by Tauri Stronghold.
5. Start the daemon with provider env vars sourced from Stronghold.
6. Show daemon/provider status in Workbench.
7. Allow a controlled desktop UI action to trigger M7 worker model completion for an existing run.
8. Document packaging, provider config, sidecar lifecycle, security boundaries, and M9 handoff.

## 4. Non-goals

M8 must not implement:

- Self-hosted personal server, cloud sync, remote accounts, or multi-device sync.
- Streaming token UI.
- Tool execution from Workbench or worker runtime.
- MCP/OpenAPI/CLI/local-script execution.
- Node/Python worker adapter.
- Embedding/document/OCR worker.
- Prompt compiler or automatic global recall.
- Full Agent Runtime claim.
- Mobile controller.
- Multi-user provider key management.
- Changing M7 Operation Ledger worker events or run lifecycle semantics.

## 5. Desktop Architecture

```text
Tauri Desktop App
  ├─ React/Vite Workbench WebView
  │    ├─ MnemeApiClient(baseUrl injected by Tauri)
  │    ├─ Provider Config UI
  │    ├─ Daemon Status UI
  │    └─ Worker Runtime trigger/list UI
  │
  ├─ Tauri Rust backend
  │    ├─ Stronghold-backed ProviderSecretStore
  │    ├─ DaemonSupervisor
  │    ├─ Tauri commands / invoke bridge
  │    └─ App lifecycle hooks
  │
  └─ Sidecar process
       └─ mneme-daemon
            ├─ local HTTP API
            ├─ SQLite Operation Ledger/read models
            └─ OpenAI-compatible provider worker env config
```

The daemon remains the Kernel/API authority. The Tauri shell is responsible for desktop lifecycle, OS integration, and secret/config handling.

## 6. Tauri App Layout

M8 adds Tauri to the existing `apps/desktop` app rather than creating a parallel UI.

Proposed layout:

```text
apps/desktop/
  package.json
  src/
    main.tsx
    api/client.ts
    app/App.tsx
    tauri/
      bridge.ts              # browser-safe wrapper around Tauri invoke
      types.ts               # desktop status/provider DTOs
  src-tauri/
    Cargo.toml
    tauri.conf.json
    build.rs                 # sidecar build/copy metadata if needed
    src/
      main.rs
      daemon.rs              # DaemonSupervisor
      provider_config.rs     # Stronghold-backed config boundary
      commands.rs            # Tauri command handlers
      state.rs               # shared app state
```

The exact file split can change during implementation, but the boundaries should remain:

- `daemon.rs` owns sidecar lifecycle.
- `provider_config.rs` owns Stronghold/key material boundary.
- `commands.rs` exposes narrow Tauri invoke commands.
- frontend `bridge.ts` hides Tauri detection and preserves browser/test fallback.

## 7. Daemon Sidecar Lifecycle

### 7.1 Sidecar model

M8 uses a bundled `mneme-daemon` sidecar binary. Tauri starts the sidecar on app startup or before the first API call.

Sidecar responsibilities:

- listen only on loopback;
- use the existing HTTP API;
- receive DB and provider config through environment variables;
- avoid logging provider API key;
- exit when the app exits, where possible.

### 7.2 Port/address selection

M8 should avoid hard-coding `127.0.0.1:3766` as the only runtime path.

Recommended behavior:

1. Tauri backend selects or reserves a loopback address.
2. It starts `mneme-daemon` with `MNEME_BIND_ADDR=<addr>`.
3. It polls `/api/v1/health` until ready or timeout.
4. It exposes `api_base_url` to the React app through a Tauri command.

Initial implementation may default to `127.0.0.1:3766` if no conflict handling is ready, but the design should include a supervisor abstraction that can later choose a free port.

### 7.3 Daemon status

The desktop app should expose a minimal daemon status DTO:

```rust
pub struct DesktopDaemonStatus {
    pub state: DaemonProcessState,
    pub api_base_url: Option<String>,
    pub last_error: Option<String>,
}

pub enum DaemonProcessState {
    NotStarted,
    Starting,
    Ready,
    Failed,
    Stopped,
}
```

Frontend equivalent:

```ts
export type DaemonProcessState = 'not_started' | 'starting' | 'ready' | 'failed' | 'stopped';

export interface DesktopDaemonStatus {
  state: DaemonProcessState;
  api_base_url?: string;
  last_error?: string;
}
```

## 8. Provider Config and Stronghold

### 8.1 Provider config model

M8 manages OpenAI-compatible provider config at the desktop layer.

Frontend-visible provider config must never expose the raw API key after save.

```rust
pub struct DesktopProviderConfigInput {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
}

pub struct DesktopProviderConfigSummary {
    pub base_url: String,
    pub model: String,
    pub timeout_secs: u64,
    pub has_api_key: bool,
}
```

TypeScript equivalent:

```ts
export interface DesktopProviderConfigInput {
  base_url: string;
  api_key: string;
  model: string;
  timeout_secs: number;
}

export interface DesktopProviderConfigSummary {
  base_url: string;
  model: string;
  timeout_secs: number;
  has_api_key: boolean;
}
```

### 8.2 Stronghold boundary

M8 uses Tauri Stronghold / encrypted local vault for provider secrets.

Design rules:

- API key is written only through Tauri backend command.
- API key is stored only in Stronghold/vault storage.
- API key is never returned to frontend after save.
- Summary command returns `has_api_key`, not the key.
- API key is never written to Operation Ledger, SQLite projections, localStorage/sessionStorage, docs examples, or logs.
- Tests should use an in-memory/test `SecretStore` instead of the real vault where possible.

Suggested trait boundary:

```rust
pub trait ProviderSecretStore: Send + Sync {
    fn save_provider_config(&self, input: DesktopProviderConfigInput) -> Result<(), SecretStoreError>;
    fn load_provider_config_summary(&self) -> Result<DesktopProviderConfigSummary, SecretStoreError>;
    fn load_provider_env(&self) -> Result<ProviderEnv, SecretStoreError>;
    fn clear_provider_config(&self) -> Result<(), SecretStoreError>;
}

pub struct ProviderEnv {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub timeout_secs: u64,
}
```

Implementation can include:

- `StrongholdProviderSecretStore` for production Tauri.
- `InMemoryProviderSecretStore` for tests.

### 8.3 Vault unlock UX

Stronghold usually requires an app-level vault password/key strategy. M8 should keep this minimal:

- first run: user sets a local vault password or app-generated vault key strategy;
- subsequent runs: user unlocks before saving/loading provider key, if required by selected Stronghold integration;
- do not silently store provider key unencrypted if vault is unavailable.

If implementation complexity becomes high, the implementation plan may split Stronghold into:

1. abstraction + test store;
2. Stronghold-backed adapter;
3. UI unlock/save flow.

But the design target remains real Tauri Stronghold-backed secure storage in M8.

## 9. Tauri Bridge Commands

M8 should expose a narrow bridge instead of letting UI access OS/process APIs directly.

Suggested commands:

```rust
#[tauri::command]
async fn get_desktop_runtime_status(...) -> Result<DesktopRuntimeStatus, DesktopError>;

#[tauri::command]
async fn ensure_daemon_started(...) -> Result<DesktopDaemonStatus, DesktopError>;

#[tauri::command]
async fn restart_daemon(...) -> Result<DesktopDaemonStatus, DesktopError>;

#[tauri::command]
async fn save_provider_config(input: DesktopProviderConfigInput, ...) -> Result<DesktopProviderConfigSummary, DesktopError>;

#[tauri::command]
async fn get_provider_config_summary(...) -> Result<DesktopProviderConfigSummary, DesktopError>;

#[tauri::command]
async fn clear_provider_config(...) -> Result<DesktopProviderConfigSummary, DesktopError>;
```

Combined frontend DTO:

```ts
export interface DesktopRuntimeStatus {
  daemon: DesktopDaemonStatus;
  provider: DesktopProviderConfigSummary;
}
```

Browser fallback:

- If not running in Tauri, `bridge.ts` returns a dev-mode status and uses `VITE_MNEME_API_BASE_URL` fallback.
- Tests can mock bridge functions.

## 10. Frontend Workbench Changes

M8 may add interactive desktop-only UI because provider config is now managed by desktop shell.

### 10.1 Desktop Runtime panel

Add a `Desktop Runtime` panel or extend `Worker Runtime` with:

- daemon state;
- API base URL;
- provider config summary (`base_url`, `model`, `timeout_secs`, `has_api_key`);
- actions:
  - start/restart daemon;
  - save provider config;
  - clear provider config.

The API key field should be write-only:

- user can enter/save a new key;
- UI never displays current key;
- after save, show `has_api_key: true`.

### 10.2 Worker completion trigger

M7 deliberately kept Workbench read-only. M8 may add a controlled trigger because provider config and daemon lifecycle are now desktop-managed.

Recommended UI:

- select or enter an existing `run_id` from current run list;
- enter prompt;
- click `Run provider completion`;
- call `MnemeApiClient.runWorkerModelCompletion(run_id, { prompt })`;
- refresh runs and worker jobs after success/failure;
- clearly label that it performs a non-streaming provider call through local daemon.

The UI must not claim streaming tokens, tool execution, automatic recall, or full Agent Runtime.

## 11. Security and Privacy

M8 security requirements:

- provider API key never appears in docs examples except variable names like `MNEME_OPENAI_API_KEY`;
- no real key in tests;
- no key returned from Tauri commands;
- no key stored in Operation Ledger or SQLite projections;
- no key written to localStorage/sessionStorage;
- no key logged by Tauri backend or daemon;
- daemon receives key only as process environment or equivalent secure runtime channel;
- error messages must not echo the key.

Provider config status may show:

- base URL;
- model;
- timeout;
- whether an API key exists.

## 12. Testing Strategy

M8 should keep automated tests offline.

Rust/Tauri backend tests:

- `InMemoryProviderSecretStore` saves/loads/clears config and never exposes raw key in summary.
- `DaemonSupervisor` command-building injects expected env vars without logging key.
- daemon status transitions can be tested with a fake process runner where possible.
- Tauri command handlers can be unit-tested with fake stores/supervisors if Tauri test harness is too heavy.

Frontend tests:

- desktop bridge fallback returns dev status in browser test environment;
- provider config form saves config and then displays `has_api_key` without rendering the key;
- daemon status panel renders ready/failed states;
- worker completion trigger calls `runWorkerModelCompletion` only on explicit user action, never automatically;
- no streaming/tool/full-agent claims in UI copy.

Integration/manual checks:

- `pnpm check` remains the main automated gate.
- Tauri build/dev command may be documented if full bundle build is too platform-sensitive for CI.
- If implemented, `pnpm --dir apps/desktop tauri:build` or equivalent should be a manual verification target.

## 13. Documentation Updates

M8 docs must update:

- `README.md` with desktop run/build instructions and provider config security note.
- `docs/architecture/milestone-8-tauri-desktop-shell.md` with:
  - scope;
  - sidecar daemon lifecycle;
  - Stronghold provider config;
  - Tauri bridge commands;
  - Workbench UI;
  - security boundary;
  - testing/verification;
  - M9 handoff.
- `docs/architecture/phase-1-foundation.md` with M8 summary.

Docs must explicitly state:

- M8 does not implement self-hosted server or sync.
- M8 does not change M7 ledger facts.
- Provider API key is stored in Stronghold and never written to ledger/projection/logs.
- M9 owns self-hosted personal server, sync, backup/restore, and remote web console.

## 14. M9 Handoff

M8 produces a local installable desktop app. M9 can then add self-hosted personal server features on top:

- device registration;
- event/object sync;
- backup and restore;
- remote Web console;
- conflict rules based on Operation Ledger.

M8 should not preempt M9 by adding remote accounts, cloud sync, or multi-device conflict resolution.

## 15. Success Criteria

M8 is successful when:

1. Repository has a Tauri 2 shell for `apps/desktop`.
2. Tauri can start/manage bundled `mneme-daemon` sidecar or has a clearly tested supervisor boundary ready for sidecar packaging.
3. React Workbench receives API base URL from desktop bridge in Tauri mode while browser/dev fallback remains usable.
4. Provider config UI writes API key through Tauri backend and never displays the saved key.
5. Stronghold-backed or Stronghold-ready secure storage stores provider secrets; tests use fake store.
6. Daemon receives provider config from desktop runtime and can use M7 worker completion path.
7. Workbench shows daemon/provider status and supports explicit worker completion trigger without streaming/tool/full-agent claims.
8. Docs explain sidecar lifecycle, Stronghold storage, security boundaries, and M9 handoff.
9. `pnpm check` passes.
10. `git diff --check` passes.

## 16. Open Implementation Risks

- Tauri 2 + Stronghold plugin exact APIs may require dependency/version adjustment during implementation.
- Cross-platform sidecar binary naming and bundling can be platform-sensitive.
- Full Tauri packaging may be heavier than current CI/test loop; M8 may need a documented manual build check if automated build is impractical in this environment.
- Stronghold unlock/password UX may need one extra design refinement if plugin constraints are stricter than expected.

These risks should be handled in the implementation plan by splitting tasks into small, reviewable slices and keeping all tests offline.
