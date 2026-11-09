# Mneme Technical Selection Design

Date: 2026-11-09
Status: Approved direction, pending user review of written spec

## 1. Product Context

Mneme is a **Personal AI Kernel（个人 AI 内核）**: a local-first, long-lived personal intelligence layer for memory, context, tasks, agents, tools, artifacts, and auditable AI work.

The product should not become a simple ChatGPT/LobeHub clone. Its technical architecture must support:

- personal long-term memory;
- local files, projects, tools, and system integration;
- agent execution with permissions and auditability;
- explainable context compilation;
- Operation Ledger / replay / traceability;
- artifact-first output management;
- user data sovereignty;
- future multi-device and self-hosted deployment.

## 2. Confirmed Architecture Direction

The selected direction is:

> **Rust-first Personal AI Kernel + Tauri/React Desktop + Event-sourced Local Data Plane + Polyglot Runtime Workers + Mneme-native Tool Protocol + Self-hosted Personal Server.**

One-line summary:

> Use Rust for the trusted local kernel and self-hosted server, React/Radix for a high-density workbench UI, event ledger plus local projections for durable memory/task/audit state, and worker protocols to connect model, tool, and multi-language ecosystems.

## 3. Top-level Runtime Shape

Mneme should ship first as:

```text
Desktop App + Local Daemon/API
```

The desktop app is the primary user interface, but the kernel is a separately bounded local service. The desktop shell must not own core business state.

Recommended shape:

```text
Tauri Desktop App
  ├─ React/TypeScript UI
  ├─ Thin Tauri native bridge
  └─ Mneme Local Kernel / Daemon
```

Long-term additional entry points:

- CLI;
- local browser UI;
- mobile controller;
- self-hosted server web console;
- automation and plugin APIs.

### Boundary Rules

1. UI does not write the database directly.
2. UI sends commands and observes event streams.
3. Kernel owns domain state, policy, ledger, and side effects.
4. CLI and future clients use the same local/public API boundaries.

## 4. Kernel Language and Role

The kernel is **Rust-first**.

Rust owns:

- long-running daemon lifecycle;
- local service boundary;
- task, run, agent, and artifact state;
- policy, approvals, and permissions;
- Operation Ledger;
- event bus;
- storage and projections;
- file watching and filesystem integration;
- process/worker supervision;
- tool execution boundaries;
- sync foundations;
- self-hosted server core reuse.

TypeScript owns UI/product iteration. Python is not a core service language; it may be used later for ML/document/OCR workers. Node.js is not the kernel, but may be a first-class worker/plugin runtime.

## 5. Interface Strategy

Mneme uses a **full dual-interface strategy**.

### 5.1 Internal Interface

Inside the kernel, modules communicate through typed services and events, not ad hoc cross-module calls.

Core services:

```text
AgentService
TaskService
RunService
ContextService
MemoryService
KnowledgeService
ArtifactService
ToolService
PolicyService
EventService
SyncService
```

Examples:

```text
RunService.start_run(...)
RunService.cancel_run(...)
ContextService.compile_context(...)
ToolService.request_tool_execution(...)
PolicyService.evaluate_effect(...)
EventService.append(...)
```

### 5.2 Event Bus vs Operation Ledger

The system distinguishes real-time event distribution from durable history.

```text
Event Bus
  - real-time
  - subscription-oriented
  - drives UI, workers, and runtime orchestration

Operation Ledger
  - append-only durable record
  - audit, replay, search, and evaluation source
  - must not be treated as disposable logs
```

Important events include:

```text
RunStarted
ContextCompiled
ModelCallStarted
ToolCallRequested
ToolCallApproved
ToolCallCompleted
ArtifactCreated
MemoryWritten
RunFailed
RunCompleted
```

### 5.3 External Interface

External/local API consumers include:

- desktop UI;
- CLI;
- browser-based local console;
- plugins;
- automation;
- future mobile/controller apps;
- self-hosted server sync bridge.

Recommended API shape:

```text
/api/v1/agents
/api/v1/tasks
/api/v1/runs
/api/v1/context
/api/v1/memory
/api/v1/artifacts
/api/v1/tools
/api/v1/events
/api/v1/settings
```

Streaming endpoints:

```text
/api/v1/events/stream
/api/v1/runs/:id/stream
```

The kernel should generate OpenAPI/JSON schema from Rust API DTOs and generate TypeScript client/types for the UI.

```text
Rust domain types
  ↓
API DTO types
  ↓
OpenAPI / JSON schema
  ↓
Generated TypeScript SDK
```

## 6. Data Layer

The selected data strategy is:

> **Event-log-first理念 + pragmatic local implementation.**

Operation Ledger is the source of truth. SQLite stores the event log and current projections. Search and vector indexes are rebuildable sidecars.

### 6.1 Storage Layout

```text
Source of Truth
  └─ Operation Ledger / append-only event log

Primary Local Store
  └─ SQLite
      - events
      - relational projections
      - settings
      - agents/tasks/runs/artifacts metadata
      - context bundle metadata
      - sync cursors

Indexes / Sidecars
  ├─ Tantivy full-text index
  ├─ VectorIndex abstraction over LanceDB / Qdrant / sqlite-vec candidates
  └─ file metadata and content chunk index

Artifact Store
  └─ local content-addressed filesystem store
```

### 6.2 Projection Tables

Initial projection candidates:

```text
events
agents
tasks
runs
artifacts
memories
knowledge_sources
context_bundles
tool_calls
approvals
```

### 6.3 Write Path

Commands should follow this shape:

```text
Command
  → validate policy
  → append event(s)
  → update projection(s)
  → publish event bus notification
  → asynchronously update search/vector sidecars
```

Search/vector sidecars are not authority. They can be rebuilt from SQLite, artifacts, and event history.

### 6.4 Concrete Data Choices

Recommended initial choices:

- SQLite for primary local persistence;
- SQLx + migrations for Rust database layer;
- Tantivy for embedded full-text search;
- VectorIndex trait before locking to LanceDB/Qdrant/sqlite-vec;
- content-addressed local filesystem store for artifacts.

## 7. Agent / Model Runtime

The selected runtime strategy is:

> **Rust Kernel as authoritative control plane; model/runtime workers as pluggable execution plane.**

Kernel owns:

- task and run lifecycle;
- context compilation;
- permission and approval policy;
- Operation Ledger;
- tool effect model;
- storage and projection;
- runtime orchestration.

Workers execute:

- model provider calls;
- streaming generation;
- provider-specific tool-call adaptation;
- embeddings;
- local model bridges;
- document/OCR/ML functions later.

### 7.1 Worker Authority Rule

Workers must not directly mutate tasks, memory, artifacts, or the ledger. They emit events/results/intents back to the kernel.

Tool flow:

```text
worker wants tool call
  → Kernel ToolService
  → PolicyService evaluates effect
  → approval if needed
  → Kernel dispatches tool execution
  → result returns to worker/model loop
  → Kernel records ledger event
```

### 7.2 Worker Types

Initial and future worker classes:

```text
RustNativeProviderWorker
  - OpenAI-compatible APIs
  - Ollama
  - basic embeddings

NodeProviderWorker
  - optional Vercel AI SDK/provider ecosystem
  - experimental providers
  - JS/TS plugin execution

PythonMLWorker
  - later
  - document parsing
  - OCR
  - local embedding/rerank
```

### 7.3 Worker Protocol

MVP protocol:

```text
stdio JSON-RPC 2.0
+ typed stream events
```

Long-term protocol:

```text
Unix domain socket / named pipe
+ ConnectRPC or equivalent typed RPC
```

The protocol should expose Mneme-native abstractions rather than provider-specific API shapes:

```text
MnemeModelRequest
MnemeModelStreamEvent
MnemeToolCallIntent
MnemeUsage
MnemeProviderError
MnemeEmbeddingRequest
```

## 8. Tool and Plugin System

The selected plugin/tool strategy is:

> **Internal Mneme-native Effect-typed Tool Protocol + compatibility adapters for MCP, OpenAPI, CLI tools, and local scripts.**

MCP is an adapter, not the internal permission model.

### 8.1 Tool Descriptor

Each tool declares:

```text
ToolDescriptor
  - id
  - name
  - description
  - input schema
  - output schema
  - effect type
  - resource scope
  - required capabilities
  - default approval policy
  - sandbox requirements
  - timeout / concurrency limit
  - audit metadata
```

Effect types include:

```text
ReadOnly
LocalWrite
ExternalNetwork
ExternalMutation
CredentialAccess
ProcessSpawn
ShellCommand
FileDelete
PaymentOrPurchase
IdentityOrAccountChange
HighImpactAction
```

### 8.2 Tool Call Intent

Agents and model workers request tool execution by producing an intent, not by directly executing side effects.

```text
ToolCallIntent
  - run_id
  - agent_id
  - tool_id
  - arguments
  - declared_effect
  - target_resources
  - model_reason
  - confidence
```

The kernel then:

```text
validate schema
→ classify/verify effect
→ evaluate policy
→ request approval if needed
→ execute in sandbox
→ record result
→ return observation
```

### 8.3 Executors

Executor classes:

```text
NativeExecutor
  - Rust built-in tools
  - filesystem read/write
  - artifact store
  - local indexes
  - internal APIs

ProcessExecutor
  - CLI commands
  - local scripts
  - Node/Python workers
  - shell-like operations under policy

McpExecutor
  - connects to MCP servers
  - maps MCP tools into Mneme descriptors

HttpExecutor
  - OpenAPI/webhook/REST tools
  - handles network/domain/credential policy
```

All executors must go through ToolService and PolicyService.

## 9. Frontend Technology

The selected frontend route is:

> **React + Vite + Radix primitives + self-owned Mneme design system.**

Recommended stack:

```text
React
Vite
TypeScript
Radix UI primitives
Mneme Design System
TanStack Router
TanStack Query or dedicated API cache layer
Zustand or Jotai for UI-only state
Monaco or CodeMirror for code/context/artifact viewers
```

Tailwind may be used as a low-level utility system, but it should not define the visual identity. The design system should own tokens, density, motion, risk states, layout rules, and component semantics.

### 9.1 UI Product Character

Mneme should feel like:

```text
Personal OS
+ research notebook
+ agent control room
+ operation ledger
+ quiet premium local tool
```

Not like a generic SaaS dashboard.

### 9.2 Initial UI Modules

Primary UI surfaces:

- Work Room;
- Agent Run Timeline;
- Operation Ledger;
- Context Inspector;
- Artifact Workspace;
- Tool Approval Panel;
- Memory / Knowledge views;
- Task Board;
- Runtime Monitor;
- Settings / Policies.

### 9.3 Frontend Boundary

Suggested source layout:

```text
apps/desktop/src
  app/
  pages/
  workbench/
  features/
    agents/
    tasks/
    runs/
    ledger/
    context/
    memory/
    tools/
    artifacts/
    settings/
  components/
  design-system/
  api/
  state/
```

Shared design package:

```text
packages/design-system
  tokens/
  primitives/
  components/
  motion/
  icons/
```

## 10. Self-hosted Personal Server

The selected cloud/sync route is:

> **Personal cloud server / self-hosted Mneme server.**

The server exists to support:

- multi-device sync;
- remote access;
- backup/restore;
- device registration;
- automation endpoints;
- future mobile/controller apps;
- future remote execution.

It should not erase local sovereignty. The local kernel remains a first-class authority for local work, permissions, files, and tools.

### 10.1 Phasing

Phase 1:

```text
Desktop + local kernel only
```

But schemas, event IDs, object IDs, API boundaries, and ledger shape should be designed for future sync.

Phase 2:

```text
Self-hosted Mneme Server
  - authenticated event/object sync
  - backup/restore
  - remote API access
  - device registration
```

Phase 3:

```text
Remote agent execution
Mobile controller
Web console
Multi-device conflict handling
```

## 11. Recommended Monorepo Layout

```text
mneme/
  apps/
    desktop/              # Tauri + React app
    web/                  # optional local/server web console later
    cli/                  # CLI entry
  crates/
    mneme-kernel/          # core domain/application services
    mneme-daemon/          # local server process
    mneme-api/             # HTTP/RPC API layer
    mneme-events/          # event bus abstractions
    mneme-storage/         # DB, migrations, repositories
    mneme-runtime/         # agent/model/tool execution runtime
    mneme-policy/          # permissions/effect typing/approval policy
    mneme-sync/            # sync foundations later
  packages/
    api-client/            # generated TS client
    api-types/             # generated TS types
    design-system/         # React/Radix design system
    plugin-sdk/            # later
  docs/
    architecture/
    superpowers/specs/
```

## 12. Initial Concrete Technology List

Desktop/UI:

```text
Tauri 2
React
Vite
TypeScript
Radix UI
TanStack Router
TanStack Query
Zustand or Jotai
Monaco or CodeMirror
```

Rust kernel/server:

```text
Tokio
Axum
Serde
SQLx
SQLite
Tracing
Tower
Tantivy
notify
uuid
chrono or time
thiserror
utoipa or aide for OpenAPI
```

Runtime/worker:

```text
JSON-RPC 2.0 over stdio for MVP workers
ConnectRPC or equivalent typed RPC for long-term workers
Node worker optional for AI/provider ecosystem
Python worker later for ML/document/OCR tasks
```

Data/indexing:

```text
SQLite event log and projections
Tantivy full-text search
VectorIndex trait, evaluate LanceDB/Qdrant/sqlite-vec
Content-addressed artifact store
```

## 13. Non-goals for the First Phase

Do not build these in phase 1 unless forced by a specific MVP requirement:

- full multi-user SaaS;
- organization/RBAC system;
- official cloud-first backend;
- complex collaborative editing;
- mobile app;
- complete plugin marketplace;
- fully generalized workflow engine;
- distributed conflict resolution beyond sync-ready IDs and ledger shape;
- replacing all provider SDK ecosystems with Rust-only implementations.

## 14. Key Risks and Mitigations

### Risk: architecture too heavy for MVP

Mitigation:

- implement thin versions of services first;
- define interfaces early, but keep behavior minimal;
- phase self-hosted server after local kernel;
- start with one or two providers and a few tools.

### Risk: polyglot workers become chaos

Mitigation:

- Rust Kernel remains the only authority;
- workers cannot write DB or ledger directly;
- all worker outputs use typed Mneme protocol;
- unified logging, versioning, and process supervision from Kernel.

### Risk: event-sourced data layer is overdesigned

Mitigation:

- store append-only events in SQLite from day one;
- maintain simple projection tables;
- avoid complex distributed event sourcing until sync phase;
- use events first for audit/replay, not theoretical purity.

### Risk: custom design system slows UI

Mitigation:

- use Radix primitives for accessibility and behavior;
- build only needed components;
- keep tokens and layout rules explicit;
- avoid building a generic component library before product screens exist.

### Risk: self-hosted server increases deployment burden

Mitigation:

- do not require server for first local MVP;
- design sync-ready IDs and ledger structure early;
- introduce server as optional backup/remote layer in phase 2.

## 15. Success Criteria

This technical selection is successful if Mneme can:

1. run as a desktop app with a real local kernel;
2. execute an agent run through Kernel-owned task/run/context/tool/ledger flow;
3. record a replayable Operation Ledger;
4. expose local API and event streams;
5. keep UI, workers, and tools from bypassing Kernel authority;
6. store core state in SQLite projections backed by append-only events;
7. show dense Work Room / Ledger / Context Inspector UI using the self-owned design system;
8. later add self-hosted server sync without rewriting local architecture.

## 16. Final Recommendation

Proceed with:

```text
Rust-first Personal AI Kernel
+ Tauri/React desktop workbench
+ typed internal services and event bus
+ HTTP/local API plus generated TypeScript SDK
+ append-only Operation Ledger with SQLite projections
+ Tantivy/search and vector sidecar abstraction
+ polyglot runtime workers under Kernel control
+ Mneme-native effect-typed Tool Protocol
+ optional self-hosted personal server in phase 2
```

This stack best preserves the product identity: **a private, auditable, extensible personal AI kernel**, not merely another chat interface or cloud agent dashboard.
