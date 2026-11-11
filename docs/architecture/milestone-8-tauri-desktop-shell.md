# Milestone 8: Tauri Desktop Shell

## Scope

Milestone 8 packages the existing React Workbench and local daemon into a Tauri desktop shell. It adds a desktop lifecycle boundary for the daemon sidecar, desktop bridge API base URL injection, provider configuration UI, Stronghold-ready provider secret storage, and an explicit Workbench control for the existing non-streaming worker completion API.

M8 keeps the Kernel, Operation Ledger, SQLite projections, worker runtime, and HTTP API owned by the daemon/kernel crates. The Tauri shell owns OS process lifecycle, browser-safe desktop bridge commands, and local provider configuration boundaries.

## Sidecar Daemon Lifecycle

The Tauri backend owns the desktop daemon lifecycle boundary. It starts or supervises a local `mneme-daemon` sidecar, keeps the daemon bound to loopback, passes `MNEME_BIND_ADDR` and provider env vars at process start, and exposes daemon status to the Workbench.

The sidecar supervisor intentionally redacts `MNEME_OPENAI_API_KEY` in debug output and status surfaces. Provider env vars are process-start inputs only; they are not persisted through Operation Ledger or SQLite projections.

Full release packaging still has a platform-sensitive follow-up: `apps/desktop/src-tauri/tauri.conf.json` keeps `externalBin` empty until the release build can copy or declare the correct per-platform `mneme-daemon` binary. M8 documents and tests the supervisor boundary, but does not claim the final sidecar binary copy/packaging step is complete.

## Provider Configuration and Stronghold

Provider config is managed through the desktop backend. The Workbench form lets the user save base URL, model, timeout, and an API key, but the API key is write-only from the frontend perspective. After save, the frontend receives only `has_api_key`, base URL, model, and timeout.

`MNEME_OPENAI_API_KEY` is never displayed after save and must not be written to frontend storage, Operation Ledger, SQLite projections, docs examples, or logs. Debug output and daemon status must redact it.

M8 uses a `StrongholdReadyProviderSecretStore` boundary so the Workbench and Tauri commands already depend on a secure-storage-shaped interface. The current implementation is ready for a real Tauri Stronghold adapter without changing the bridge or UI contract; completing the platform vault/copy details is a release follow-up, not a Kernel data-model change.

## Tauri Bridge

The Workbench uses a small browser-safe bridge. In Tauri mode, bridge commands return desktop runtime status, daemon status, provider config summary, and the API base URL used by the typed client. In browser development mode, the bridge falls back to `VITE_MNEME_API_BASE_URL` or `http://127.0.0.1:3766`.

This keeps React code usable in both desktop and browser development while avoiding direct frontend access to secrets or daemon process internals.

## Workbench

The Workbench shows desktop runtime status, daemon status, provider config summary, and a write-only provider key form. It also adds an explicit user-triggered non-streaming worker completion control for an existing run by calling the already-defined worker completion API.

Workbench does not implement streaming token UI or tool execution in M8. Worker completion remains an explicit non-streaming action, and the daemon/kernel remain authoritative for run lifecycle and worker job records.

## Non-goals

M8 does not implement self-hosted server, cloud sync, streaming token UI, tool execution, Node/Python worker adapters, embedding/document/OCR workers, prompt compiler, automatic global recall, remote console, or a full Agent Runtime.

M8 also does not claim final cross-platform sidecar release packaging while `externalBin`/binary copy remains deferred.

## Verification

Automated verification remains:

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
git status --short
```

Full desktop bundle verification can be platform-specific and should use the Tauri command once sidecar binary copy/declaration is ready for the target release platform.

## M9 Handoff

Milestone 9 owns self-hosted personal server, device registration, event/object sync, backup/restore, remote Web console, and Ledger-based conflict rules.
