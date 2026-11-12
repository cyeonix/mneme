# Mneme Phase 2 Workbench Productization Design

Date: 2026-11-12
Status: Approved for written spec after Phase 1 local validation

## 1. Decision

Phase 2 focuses on **Workbench Productization**: turning the Phase 1 Mneme Workbench from a mostly read-only inspector into a usable local-first control room for daily personal AI work.

The selected direction is:

> Make the existing local Kernel, daemon/API, Operation Ledger, Artifact/Context/Search/Memory inspectors, Worker Runtime, desktop shell, and sync foundations usable from the Workbench without requiring curl for normal workflows.

Phase 2 does not replace the Rust-first Kernel architecture. The Workbench remains a client of the local API and must not write SQLite directly. The Operation Ledger remains the audit source of truth.

## 2. Product Goal

By the end of Phase 2, a user should be able to open Mneme and complete a practical local workflow from the UI:

1. create a run;
2. inspect the run timeline;
3. create and preview artifacts;
4. create context bundles from artifacts/events;
5. create and chunk knowledge sources;
6. record memories manually;
7. run deterministic keyword search;
8. trigger an explicit non-streaming provider completion for a selected run;
9. understand errors, empty states, and missing local setup without reading terminal logs.

The product goal is not to build a full autonomous Agent Runtime yet. Phase 2 prepares the user-facing control surface and data model interactions that later Agent Runtime work will use.

## 3. Current Context

Phase 1 established:

- Rust-first `mneme-kernel` with domain types, Operation Ledger, SQLite projections, and service boundaries.
- Local `mneme-api` routes and `mneme-daemon` for HTTP access.
- TypeScript `MnemeApiClient` with methods for runs, lifecycle events, tool events, artifacts, context bundles, knowledge sources, chunks, memories, search, retrieval traces, worker completions, and worker jobs.
- React Workbench sections for Kernel status, Operation Ledger count, Desktop Runtime, Runs, Tool Activity, Artifact Workspace, Context Inspector, Knowledge Search, Memory Inspector, Retrieval Traces, and Worker Runtime.
- Tauri desktop shell with write-only provider config and daemon supervision boundary.
- Self-hosted sync foundations through `mneme-server` and `mneme-sync`.

Current Workbench limitations:

- Runs are listed but cannot be created from the UI.
- There is no run detail/timeline view.
- Artifact, context, knowledge, memory, and search workflows are mostly read-only.
- Worker completion requires an existing run and is not integrated into a run detail workflow.
- Tool Activity is only an event view; no real tool execution is in scope.
- Sync/backup server foundations are implemented but not part of the normal Workbench workflow.
- Error handling is mostly a generic banner.

Recent runtime fixes before this design:

- local Workbench CORS support for `http://localhost:5173` and `http://127.0.0.1:5173`;
- durable local artifact content storage under `.mneme/artifacts`;
- stale artifact metadata returns `content: null` instead of failing the page with 500.

## 4. Phase 2 Scope

Phase 2 includes five productization pillars.

### 4.1 Run Creation and Run Detail Timeline

The Workbench must let the user create runs from the UI and inspect each run as a coherent timeline.

Included:

- `New Run` form with instruction input.
- Run list selection.
- Run detail panel for the selected run.
- Timeline grouped from Operation Ledger events for that run.
- Lifecycle actions for manual model request, assistant message, complete, and fail where useful for debugging and manual workflows.
- Status-specific empty states and terminal-state messaging.

Not included:

- autonomous multi-step Agent Runtime;
- streaming event subscriptions;
- real-time WebSocket/SSE updates.

### 4.2 Artifact and Context Authoring

The Workbench must allow artifact and context workflows without curl.

Included:

- Create artifact form for `text`, `markdown`, and `json` artifacts.
- MIME defaults based on artifact kind.
- Artifact preview with clear handling for `content: null`.
- Artifact association with a selected run.
- Context bundle creation from selected artifact IDs and event IDs.
- Readable context bundle detail view.

Not included:

- binary upload UX;
- drag-and-drop files;
- rich markdown editor;
- prompt compile UI;
- automatic context selection.

### 4.3 Knowledge, Memory, and Search Console

The Workbench must turn the Phase 1 search/memory/knowledge APIs into a manual local console.

Included:

- Create manual knowledge source for a selected run.
- Create knowledge source from artifact or context bundle.
- Chunk a knowledge source.
- Record memory manually with kind, title, body, optional knowledge source, and confidence.
- Keyword search form with selected run, query, and limit.
- Search results with source title, chunk text, score, and reason.
- Retrieval trace list linked to the performed search.

Not included:

- embeddings;
- vector database;
- Tantivy production index;
- OCR/PDF/web ingestion;
- automatic long-term memory;
- automatic agent recall;
- prompt-injection filtering.

### 4.4 Worker Runtime UX Integration

The Workbench must make the existing explicit provider completion more understandable and useful.

Included:

- Worker completion action from selected run detail.
- Prompt textarea scoped to the selected run.
- Clear provider configuration status and missing-key guidance.
- Worker job status display near the run detail.
- Refresh affected data after completion: runs, worker jobs, events, and artifact/search/memory panels as needed.

Not included:

- streaming token UI;
- tool execution;
- automatic retrieval context compilation;
- background job polling beyond explicit refresh or existing load cycle;
- provider key display after save.

### 4.5 Workbench UX Structure, Error Handling, and Local Data Hygiene

Phase 2 should make the current single-page dashboard understandable as a product surface.

Included:

- Clear top-level layout for primary workflow: Runs → Detail → Artifacts/Context → Knowledge/Memory/Search → Worker.
- Better empty states that tell the user what action to take next.
- Local API error messages mapped to user-facing copy.
- Loading states for async actions.
- Manual refresh control.
- Preserve `.mneme/` as local runtime data and never commit sample databases/artifacts.

Not included:

- full design system overhaul;
- authentication;
- multi-user account model;
- hosted cloud service;
- mobile UI;
- remote Web Console.

## 5. Non-goals

Phase 2 explicitly does not implement:

- full autonomous Agent Runtime;
- real tool execution;
- shell/file/network tool side effects;
- MCP/OpenAPI/CLI tool adapters;
- streaming model responses;
- embeddings/vector search;
- OCR/PDF/web ingestion;
- automatic long-term memory or automatic recall;
- CRDT sync;
- official cloud hosting;
- end-to-end encrypted sync;
- production Tauri release packaging;
- mobile controller app;
- direct SQLite writes from the frontend.

These are later phases. Phase 2 should not weaken Phase 1 safety boundaries to create a more impressive demo.

## 6. Architecture

Phase 2 keeps the existing architecture:

```text
React Workbench
  -> MnemeApiClient
  -> HTTP /api/v1/*
  -> mneme-api routes
  -> mneme-kernel services
  -> Operation Ledger + SQLite projections + artifact content store
```

Key rule:

> UI actions are commands through the API. UI reads are projections and ledger-derived views. The UI never mutates storage directly.

### 6.1 Frontend Structure

The current `App.tsx` can remain the shell at the start of Phase 2, but Phase 2 should split large UI responsibilities into focused components as features land.

Recommended component boundaries:

```text
apps/desktop/src/app/
  App.tsx
  data/useWorkbenchData.ts
  runs/RunList.tsx
  runs/NewRunForm.tsx
  runs/RunDetail.tsx
  runs/RunTimeline.tsx
  artifacts/ArtifactWorkspace.tsx
  artifacts/CreateArtifactForm.tsx
  context/ContextInspector.tsx
  knowledge/KnowledgeConsole.tsx
  memory/MemoryConsole.tsx
  search/SearchConsole.tsx
  worker/WorkerRuntimePanel.tsx
  shared/ErrorBanner.tsx
  shared/EmptyState.tsx
```

The exact file split may be adjusted during implementation, but each component should have one purpose and testable props.

### 6.2 Data Loading Pattern

The Workbench should use a centralized load/refresh function that can be called after every mutation.

Minimum data groups:

- health;
- events;
- runs;
- artifact previews;
- context bundles;
- knowledge sources;
- document chunks;
- memories;
- retrieval traces;
- worker jobs;
- desktop runtime status.

Mutation handlers should refresh only the affected groups when possible, but correctness is more important than premature optimization in Phase 2.

### 6.3 Timeline Model

The run detail timeline is derived from `OperationEvent[]` filtered by `run_id`.

Timeline item categories:

- `RunStarted`;
- model request;
- assistant message;
- run completed;
- run failed;
- tool request/approval/denial/result/failure;
- artifact created;
- context bundle created;
- knowledge source created;
- document chunk created;
- memory recorded;
- retrieval trace recorded;
- worker job started/completed/failed.

Unknown event kinds should still render as JSON/debug rows rather than disappearing.

## 7. Data Flow

### 7.1 Create Run Flow

```text
User submits instruction
  -> client.createRun(instruction)
  -> API creates RunStarted event and run projection
  -> Workbench refreshes runs/events
  -> newly created run becomes selected
  -> RunDetail shows timeline
```

Validation:

- blank instruction is blocked in UI;
- API errors are displayed without losing typed input.

### 7.2 Create Artifact Flow

```text
User selects run + artifact kind + content
  -> client.createArtifact(runId, request)
  -> kernel writes content-addressed body + artifact projection + ledger event
  -> Workbench refreshes artifacts/events/runs
  -> artifact preview appears in Artifact Workspace and selected run timeline
```

If an existing artifact has missing content:

- display metadata;
- show `Content is unavailable for this artifact. It may have been created before durable artifact storage was enabled.`;
- do not fail the whole page.

### 7.3 Knowledge and Search Flow

```text
User creates or selects knowledge source
  -> optionally chunk source
  -> user runs keyword search
  -> search returns hits and retrieval_trace_id
  -> Workbench displays hit reasons and source references
  -> Retrieval Traces updates
```

Search remains deterministic keyword search. The UI should say that explicitly.

### 7.4 Worker Completion Flow

```text
User selects run detail
  -> enters prompt
  -> client.runWorkerModelCompletion(runId, { prompt })
  -> daemon uses configured provider or returns configuration error
  -> RunService records lifecycle/job events
  -> Workbench refreshes run detail, worker jobs, and events
```

Provider secrets remain write-only and must not appear in logs, Operation Ledger, projections, frontend storage, or docs examples.

## 8. Error Handling

Phase 2 should replace generic failure UX with actionable errors.

Examples:

- daemon unreachable: `Local daemon is not reachable. Start the daemon and refresh.`
- missing provider key: `Provider key is not configured. Save a provider key in Desktop Runtime before running completion.`
- blank form input: inline field error;
- artifact content missing: metadata card with missing-content explanation;
- unknown API error: preserve status code and endpoint context without exposing secrets.

Errors should not clear existing loaded data unless the underlying data actually changed.

## 9. Testing Strategy

Phase 2 should continue the existing TDD pattern.

Required test types:

### 9.1 API Client Tests

Add or update `apps/desktop/src/api/client.test.ts` for any newly used client methods or request shapes.

### 9.2 App/Component Tests

Use existing React/Vitest testing style.

Core expectations:

- New Run form calls `client.createRun` and refreshes/selects the new run.
- Run detail timeline renders lifecycle, artifact, memory, retrieval, worker, and unknown event rows.
- Artifact create form blocks blank content/title and calls `client.createArtifact` with expected DTO.
- Missing artifact content renders friendly copy, not an error banner.
- Knowledge source creation/chunk/search flows call the correct client methods.
- Memory form validates confidence bounds before submission.
- Worker completion is scoped to selected run and does not call API for blank prompt.
- Provider key is never rendered after save.

### 9.3 Rust Verification

Phase 2 is frontend-heavy at first, but full verification remains:

```bash
. "$HOME/.cargo/env" && pnpm check
. "$HOME/.cargo/env" && cargo test --workspace
pnpm --dir apps/desktop test
pnpm --dir apps/desktop check
```

The final gate for each task should include `git diff --check`.

## 10. Milestone Breakdown

### Milestone 10: Run Creation and Run Detail Timeline

Deliverables:

- New Run form.
- Run selection.
- Run detail panel.
- Timeline derived from Operation Ledger events.
- Manual lifecycle debug controls if they fit without clutter.
- Tests for create run, selection, and timeline rendering.

Acceptance:

- A user can create a run from the UI without curl.
- The new run is selected after creation.
- Events for that run appear in a readable timeline.

### Milestone 11: Artifact and Context Authoring

Deliverables:

- Create artifact form.
- Artifact preview and missing-content handling.
- Context bundle creation from selected artifacts/events.
- Context bundle detail display.
- Tests for artifact/context flows.

Acceptance:

- A user can create a text/markdown/json artifact from the UI.
- The artifact appears in the run timeline and Artifact Workspace.
- Missing content does not break page load.

### Milestone 12: Knowledge, Memory, and Search Console

Deliverables:

- Manual knowledge source form.
- Artifact/context-to-knowledge actions.
- Chunk source action.
- Record memory form.
- Keyword search form and results.
- Retrieval trace display linked to searches.

Acceptance:

- A user can add local knowledge and memory from the UI.
- A user can run keyword search and understand why results matched.
- Retrieval trace appears after search.

### Milestone 13: Worker Runtime UX Integration

Deliverables:

- Worker completion moved closer to selected run detail.
- Provider configuration guidance.
- Worker job status near run context.
- Refresh behavior after completion.
- Tests for blank prompt/no run/missing config/success path.

Acceptance:

- A user with provider config can trigger a non-streaming completion from a selected run.
- The result updates run lifecycle and worker job displays.
- No provider key is displayed or persisted in frontend storage.

### Milestone 14: Workbench UX Hardening and Docs

Deliverables:

- Better loading states, empty states, and errors.
- Manual refresh control.
- README updated to describe UI-first workflows instead of curl-first workflows.
- Architecture doc for Phase 2 Workbench productization.
- Final verification.

Acceptance:

- A new user can open Workbench and understand the main workflow.
- README documents the UI path and keeps curl examples as developer fallback.
- Full verification passes.

## 11. Success Criteria

Phase 2 is complete when:

1. normal local workflows do not require curl;
2. run creation and run detail timeline are usable from the Workbench;
3. artifact/context/knowledge/memory/search workflows are usable from the Workbench;
4. worker completion is integrated with selected run detail;
5. user-facing error states explain next actions;
6. Operation Ledger remains source of truth;
7. provider secrets remain write-only and are not exposed;
8. tests cover all new UI mutation flows;
9. `pnpm check` and workspace tests pass;
10. docs describe the Phase 2 UI-first workflow.

## 12. Deferred Work After Phase 2

After Phase 2, Mneme should be ready for one of these next design tracks:

- Agent Runtime MVP: automatic context assembly, model step execution, assistant outputs, and artifact/memory generation.
- Tool Execution MVP: effect-typed safe local tools with approval and audit.
- Sync/Backup UI: connect M9 self-hosted server workflows to Workbench.
- Search and ingestion expansion: embeddings, OCR/PDF/web ingestion, and stronger retrieval.
- Desktop release hardening: sidecar packaging, first-run onboarding, logs viewer, and release build verification.

The recommended next track after Phase 2 is **Agent Runtime MVP**, because Phase 2 creates the UI surfaces needed to observe and control it.
