# Milestone 7: Worker Runtime Provider

Milestone 7 adds the Phase 1 worker runtime provider MVP: a daemon-owned worker runtime service that can call an OpenAI-compatible provider for a non-streaming chat completion, record the worker job lifecycle, advance the run lifecycle through the Kernel service boundary, and expose worker jobs to the API client and Workbench.

## Scope

M7 includes:

- A `ProviderWorker` boundary with a mock provider for tests and an `OpenAiCompatibleProviderWorker` for configured local daemon runs.
- A real OpenAI-compatible non-streaming chat completion path when configured with provider environment variables.
- Worker job events and projection records for `model_completion` jobs with `started`, `completed`, and `failed` status.
- A `WorkerRuntimeService` that coordinates worker job events, provider calls, and run lifecycle changes.
- HTTP endpoints and TypeScript client methods for triggering worker model completion and listing worker jobs.
- A read-only Workbench `Worker Runtime` section that shows worker job history.

M7 remains an MVP worker runtime slice. It does not claim a full Agent Runtime, streaming token UI, tool execution, desktop-native provider management, embeddings, document ingestion workers, automatic recall, or prompt compiler integration.

## Event Flow

The worker model completion flow is:

```text
POST /api/v1/runs/:run_id/worker-model-completion
  → WorkerRuntimeService::run_model_completion(run_id, prompt)
  → WorkerJobStarted event append
  → worker_jobs projection apply
  → RunService::request_model(run_id, prompt)
  → ProviderWorker::complete(...)
  → WorkerJobCompleted or WorkerJobFailed event append
  → worker_jobs projection apply
  → RunService records assistant/final run lifecycle outcome
```

Successful completions append a worker job start event, record the model request through `RunService`, call the configured provider, append a worker job completion event, then record the assistant message and complete the run through `RunService`.

Provider failures append a worker job failure event, project the failure into `worker_jobs`, and fail the run through `RunService`. Provider configuration, timeout, non-2xx status, HTTP errors, invalid requests, and malformed responses are mapped to worker runtime errors before returning an HTTP status.

## Provider Configuration

The daemon builds the OpenAI-compatible provider from environment variables:

- `MNEME_OPENAI_BASE_URL`: provider base URL, defaulting to `https://api.openai.com/v1`.
- `MNEME_OPENAI_API_KEY`: bearer token for the provider. If absent or empty, worker model completion returns a missing configuration error instead of calling the network.
- `MNEME_OPENAI_MODEL`: model name, defaulting to `gpt-4o-mini`.
- `MNEME_OPENAI_TIMEOUT_SECS`: request timeout in seconds, defaulting to `30`.

When configured, M7 uses real OpenAI-compatible non-streaming chat completion. The provider sends `POST {base_url}/chat/completions` with a user message and `"stream": false`, parses `choices[0].message.content`, and preserves optional usage metadata in the provider response boundary.

Tests use mock provider and do not require external network or real API key. The mock provider is the default test dependency for worker runtime service/API coverage so automated verification does not depend on provider availability or credentials.

`MNEME_OPENAI_API_KEY` is never written to ledger or projection. It is read from the daemon environment and used only as an outbound bearer token by the provider HTTP client; worker job records include worker ids, run ids, status, optional model name, optional error text, and timestamps, not provider secrets.

## Worker Runtime Boundary

Worker runtime writes run lifecycle only via `RunService`. The runtime may append worker job events to the Operation Ledger and apply them to the worker job projection, but run lifecycle changes stay behind the existing Kernel service boundary:

- `request_model` records the model request lifecycle event.
- `record_assistant_message` records assistant output on success.
- `complete_run` marks the run complete on success.
- `fail_run` marks the run failed on provider/runtime failure.

This preserves the Phase 1 rule that HTTP routes and worker orchestration do not directly mutate run projections. The Operation Ledger remains the source of truth, while `runs` and `worker_jobs` are rebuildable read models.

## HTTP API

M7 exposes two worker runtime endpoints:

- `POST /api/v1/runs/:run_id/worker-model-completion` with body `{"prompt":"..."}` starts a model completion worker job for an existing run.
- `GET /api/v1/worker-jobs` lists projected worker job records ordered newest first.

The trigger endpoint validates the run id and prompt, returns `run_id` and `worker_job_id` on success, and maps runtime errors to HTTP status codes. Missing provider configuration returns service unavailable, provider timeout returns gateway timeout, provider gateway failures return bad gateway, unknown runs return not found, and invalid input returns bad request.

The list endpoint returns `worker_jobs`, where each record includes `worker_job_id`, `worker_id`, `run_id`, `kind`, `status`, `model`, `error`, `started_at`, and `completed_at`.

## Workbench

The Workbench consumes `GET /api/v1/worker-jobs` through the TypeScript client and renders a read-only `Worker Runtime` panel. It displays job kind, status, run id, optional model, optional error, start time, and completion time.

Workbench does not provide streaming token UI or tool execution. It also does not expose provider key configuration, model selection, worker process management, or a button that triggers worker model completion. Provider keys are configured outside the Workbench in M7.

## Run-scoping Rule

Worker-driven retrieval/recall defaults to `run_id` scope; global recall must be explicit in a future milestone. Any future worker path that retrieves knowledge, memory, chunks, traces, or context for a model prompt should start from the current run id and avoid implicit global recall unless a later milestone defines an explicit opt-in API, policy, and UI.

## Non-goals

M7 does not provide:

- Streaming token UI or streaming provider transport.
- Tool execution from the worker runtime or Workbench.
- Desktop-native provider configuration UI.
- Desktop process supervision, bundled provider processes, or Tauri packaging.
- Node/Python worker adapters.
- Embedding jobs, vector indexing, OCR/PDF/web ingestion, or background document ingestion.
- Automatic long-term memory, automatic global recall, memory ranking, semantic ranking, or prompt compiler integration.
- Cloud sync, remote accounts, or multi-user provider key management.

M8 owns desktop-native provider config and process management.

## Verification

Documentation and implementation verification for this milestone should include:

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
git status --short --branch
```

Expected result: whitespace check passes, docs scan prints `docs scan hits: []`, `pnpm check` passes, and git status contains only the intended documentation changes before the documentation commit.
