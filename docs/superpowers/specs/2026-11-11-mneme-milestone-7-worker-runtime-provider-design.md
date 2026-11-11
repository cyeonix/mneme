# Mneme Milestone 7 Worker Runtime Provider Design

## 1. 背景

Milestone 7 进入 Worker Runtime Protocol。用户已选择 **LLM Provider Worker first**，并进一步要求 M7 必须接入真实 OpenAI-compatible 调用。

因此本设计采用：

> **真实 OpenAI-compatible 非流式 Provider Worker MVP + Mneme-native Worker Runtime 边界。**

M7 的目标不是把所有 worker 能力一次做完，而是在 Rust Kernel 权威控制面下，完成一条真实 provider 调用的可验证闭环：

```text
Run
  → WorkerRuntimeService
  → OpenAiCompatibleProviderWorker
  → provider HTTP /v1/chat/completions
  → RunService.record_assistant_message / complete_run / fail_run
  → Operation Ledger + runs projection
```

Worker 不能直接写 SQLite、Operation Ledger、runs projection、knowledge/memory/retrieval projection。所有状态变化必须通过 Kernel service boundary 完成。

## 2. 用户确认的范围

M7 必须包含：

1. 真实 OpenAI-compatible provider call。
2. Mneme-native worker runtime / provider worker 抽象。
3. 非流式 chat completion MVP。
4. 运行时配置与错误处理。
5. worker lifecycle / runtime error 事件化或投影化。
6. 与现有 `RunService` 生命周期事件衔接。
7. 明确 run-scoping：worker-driven retrieval / recall 默认以 `run_id` 为边界；跨 run / global recall 后续必须显式 opt-in。

M7 必须避免把后续 milestone 范围提前吞入：

- 不做 streaming token UI。
- 不做 tool-call execution。
- 不做 MCP / OpenAPI / CLI / local script execution。
- 不做 Tauri process management。
- 不做 Node/Python worker 真进程管理。
- 不做 embedding worker、document parser、OCR worker。
- 不做 prompt compilation 或自动 agent recall。
- 不在代码或文档中声称完整 Agent Runtime 已完成。

## 3. 设计原则

### 3.1 Kernel 是控制面

Kernel 负责：

- run lifecycle；
- Operation Ledger；
- provider worker 配置校验；
- runtime job lifecycle；
- provider result 转换成 Mneme lifecycle event；
- runtime error 的可追踪记录。

Provider worker 只负责执行外部 HTTP 调用并返回 typed result。

### 3.2 Provider shape 不泄漏到上层

OpenAI-compatible HTTP shape 只能存在于 provider worker 内部。Kernel 和 API 暴露 Mneme-native 类型：

```rust
pub struct ModelCompletionRequest {
    pub run_id: RunId,
    pub prompt: String,
    pub model: String,
}

pub struct ModelCompletionResponse {
    pub content: String,
    pub usage: Option<ModelUsage>,
}
```

上层不直接操作 OpenAI `choices` / `messages` / `usage` JSON。

### 3.3 真实 provider 但测试默认离线

M7 需要真实 OpenAI-compatible provider 支持，但 CI / 本地默认测试不能依赖外网或真实 API key。

因此实现必须同时提供：

- `MockProviderWorker`：确定性测试 worker。
- `OpenAiCompatibleProviderWorker`：真实 HTTP provider worker。
- HTTP client abstraction 或 mockable transport，让 provider request/response mapping 可以离线测试。

真实调用只在显式配置 provider base URL 与 API key 时启用。

## 4. Worker Runtime 核心类型

### 4.1 Worker IDs

新增强类型 ID：

```rust
pub struct WorkerId(Uuid);
pub struct WorkerJobId(Uuid);
```

`WorkerId` 标识 provider worker instance / logical worker。`WorkerJobId` 标识一次 runtime job。

### 4.2 Worker job kind

M7 先定义最小可扩展枚举：

```rust
pub enum WorkerJobKind {
    ModelCompletion,
}
```

Embedding、DocumentParse、Reindex 可以作为后续 variant，但 M7 不实现。

### 4.3 Worker job status

```rust
pub enum WorkerJobStatus {
    Started,
    Completed,
    Failed,
}
```

### 4.4 Worker runtime records

```rust
pub struct WorkerJobRecord {
    pub worker_job_id: WorkerJobId,
    pub worker_id: WorkerId,
    pub run_id: RunId,
    pub kind: WorkerJobKind,
    pub status: WorkerJobStatus,
    pub model: Option<String>,
    pub error: Option<String>,
    pub started_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
}
```

## 5. Operation Ledger events

M7 增加 runtime 事件，但仍保留 `OperationEvent { run_id }` 约束。

```rust
OperationEventKind::WorkerJobStarted {
    worker_job_id: WorkerJobId,
    worker_id: WorkerId,
    kind: WorkerJobKind,
    model: Option<String>,
}

OperationEventKind::WorkerJobCompleted {
    worker_job_id: WorkerJobId,
    worker_id: WorkerId,
    kind: WorkerJobKind,
}

OperationEventKind::WorkerJobFailed {
    worker_job_id: WorkerJobId,
    worker_id: WorkerId,
    kind: WorkerJobKind,
    reason: String,
}
```

Model completion 的内容不直接写在 worker event 中。内容通过既有 `RunAssistantMessage` 与 `RunCompleted` / `RunFailed` 生命周期事件写入，保持 agent/run lifecycle 的单一边界。

## 6. Provider Worker trait

```rust
#[async_trait]
pub trait ProviderWorker: Send + Sync {
    fn worker_id(&self) -> WorkerId;

    async fn complete(
        &self,
        request: ModelCompletionRequest,
    ) -> Result<ModelCompletionResponse, ProviderWorkerError>;
}
```

错误类型：

```rust
pub enum ProviderWorkerError {
    MissingConfiguration,
    InvalidRequest,
    Http(String),
    ProviderStatus { status: u16, body: String },
    MalformedResponse(String),
    Timeout,
}
```

M7 不要求 provider worker 执行工具调用。若 OpenAI-compatible 响应含 tool calls，MVP 可以返回 `MalformedResponse` 或记录 unsupported provider response，后续 M7+ / M8+ 再接 Tool Protocol。

## 7. OpenAI-compatible provider MVP

### 7.1 配置来源

Daemon 通过环境变量配置真实 provider：

```text
MNEME_OPENAI_BASE_URL=https://api.openai.com/v1
MNEME_OPENAI_API_KEY=...
MNEME_OPENAI_MODEL=gpt-4o-mini
MNEME_OPENAI_TIMEOUT_SECS=30
```

要求：

- `MNEME_OPENAI_API_KEY` 不得写入日志、ledger、projection 或 test fixture。
- `MNEME_OPENAI_BASE_URL` 默认可以是 `https://api.openai.com/v1`。
- 如果缺少 API key，daemon 仍能启动，但真实 provider route/job 返回配置错误。
- 测试不得依赖真实 API key。

### 7.2 HTTP request shape

Provider worker 内部调用：

```http
POST {base_url}/chat/completions
authorization: Bearer <api-key>
content-type: application/json

{
  "model": "<model>",
  "messages": [
    { "role": "user", "content": "<prompt>" }
  ],
  "stream": false
}
```

M7 只支持单 user message prompt，不做 system prompt builder、context compiler 或 multi-message conversation。

### 7.3 HTTP response mapping

从 OpenAI-compatible response 读取：

```json
{
  "choices": [
    { "message": { "role": "assistant", "content": "..." } }
  ],
  "usage": {
    "prompt_tokens": 1,
    "completion_tokens": 2,
    "total_tokens": 3
  }
}
```

映射为：

```rust
ModelCompletionResponse {
    content,
    usage: Some(ModelUsage { prompt_tokens, completion_tokens, total_tokens }),
}
```

若 `choices[0].message.content` 缺失或为空，返回 `MalformedResponse`。

## 8. WorkerRuntimeService

M7 新增 service：

```rust
pub struct RunModelWithWorkerOutcome {
    pub run_id: RunId,
    pub worker_job_id: WorkerJobId,
}

#[async_trait]
pub trait WorkerRuntimeService: Send + Sync {
    async fn run_model_completion(
        &self,
        run_id: RunId,
        prompt: String,
    ) -> Result<RunModelWithWorkerOutcome, WorkerRuntimeServiceError>;

    async fn list_worker_jobs(&self) -> Result<Vec<WorkerJobRecord>, WorkerRuntimeServiceError>;
}
```

执行顺序：

1. 校验 run 存在且仍为 running。
2. 校验 prompt 非空。
3. 追加 `WorkerJobStarted`。
4. 调用 `RunService.request_model(run_id, prompt.clone())`。
5. 调用 provider worker。
6. 成功：
   - 追加 `WorkerJobCompleted`；
   - 调用 `RunService.record_assistant_message(run_id, content)`；
   - 调用 `RunService.complete_run(run_id, summary)`，summary 可为 assistant content 的短截断或固定 `Model completion finished`。
7. 失败：
   - 追加 `WorkerJobFailed`；
   - 调用 `RunService.fail_run(run_id, reason)`。

如果任一步骤失败，不得绕过 RunService 直接写 runs projection。

## 9. SQLite projection

新增表：

```sql
create table if not exists worker_jobs (
  worker_job_id text primary key,
  worker_id text not null,
  run_id text not null,
  kind text not null,
  status text not null,
  model text,
  error text,
  started_at text not null,
  completed_at text
);

create index if not exists idx_worker_jobs_run_id_started_at
  on worker_jobs(run_id, started_at);
```

Projection 必须可由 `OperationEventKind::WorkerJobStarted / Completed / Failed` 重建。

## 10. HTTP API

M7 暴露最小 API：

```text
POST /api/v1/runs/:run_id/worker-model-completion
GET  /api/v1/worker-jobs
```

DTO：

```rust
pub struct RunWorkerModelRequest {
    pub prompt: String,
}

pub struct RunWorkerModelResponse {
    pub run_id: String,
    pub worker_job_id: String,
}

pub struct WorkerJobsResponse {
    pub worker_jobs: Vec<WorkerJobRecord>,
}
```

错误映射：

- invalid UUID / blank prompt → `400 BAD_REQUEST`。
- unknown run → `404 NOT_FOUND`。
- terminal run → `400 BAD_REQUEST`。
- provider missing config → `503 SERVICE_UNAVAILABLE`。
- provider HTTP/status/malformed response/timeout → `502 BAD_GATEWAY` 或 `504 GATEWAY_TIMEOUT`。
- ledger/projection/storage → `500 INTERNAL_SERVER_ERROR`。

## 11. TypeScript client 与 Workbench

TS client 新增：

```ts
runWorkerModelCompletion(runId: string, request: RunWorkerModelRequest): Promise<RunWorkerModelResponse>
listWorkerJobs(): Promise<WorkerJobsResponse>
```

Workbench M7 只读/轻交互范围：

- 可以提供一个最小按钮或 test-only path 发起 worker model completion，但必须清楚标记需要 provider config。
- 必须显示 worker jobs：status、kind、run_id、model、error、started_at/completed_at。
- 不做 streaming token view。
- 不做 provider key 输入 UI。
- 不做 tool execution UI。

如果实现计划为了控制范围选择只展示 worker jobs、API/client 先提供触发能力，也可接受；但 M7 成功标准必须包含真实 provider service 的离线可测 HTTP mapping。

## 12. Run-scoping 语义

M6 broad review 指出 run-scoping 需要在 M7 前明确。M7 规定：worker-driven retrieval/recall 默认以 `run_id` 为边界。

具体规则：

1. Worker-driven model completion 始终绑定单个 `run_id`。
2. WorkerRuntimeService 只能读取/写入该 run 的 lifecycle。
3. 若后续 worker-driven retrieval/recall 接入 M6 search/memory，默认必须传入并过滤 `run_id`。
4. 跨 run / global recall 必须使用显式字段，例如 `scope: Global` 或 `allow_cross_run: true`，不得隐式发生。

M7 本身不需要重写 M6 search SQL，但必须在文档和 service 边界中固定这个原则。

## 13. Tests

M7 应包含这些测试层次：

- Kernel unit tests：
  - worker IDs display/parse。
  - worker job events roundtrip。
  - provider response mapping。
  - missing config / provider error mapping。
  - mock provider success drives RunService lifecycle events。
  - provider failure records `WorkerJobFailed` and fails run。
  - SQLite worker job projection rebuild。
- API tests：
  - `POST /api/v1/runs/:run_id/worker-model-completion` success with mock provider。
  - blank prompt 400。
  - unknown run 404。
  - provider missing config 503 或 mapped error。
  - `GET /api/v1/worker-jobs` lists jobs。
- Desktop tests：
  - client calls correct paths。
  - Workbench displays worker jobs without claiming streaming/tool execution。
- Full verification：
  - `. "$HOME/.cargo/env" && pnpm check`
  - `git diff --check`

## 14. Success criteria

Milestone 7 完成时必须满足：

1. Kernel 导出 worker runtime / provider worker 类型与 service。
2. OpenAI-compatible provider worker 能构造真实 HTTP request 并解析真实兼容 response。
3. 默认测试不依赖外网或真实 API key。
4. WorkerRuntimeService 通过 RunService 写 lifecycle，不直接写 runs projection。
5. Worker job started/completed/failed 可记录并可由 ledger 重建 projection。
6. Daemon 支持环境变量配置真实 provider，但不泄露 API key。
7. HTTP API 与 TS client 暴露最小 worker model completion vertical slice。
8. Workbench 至少能显示 worker job 状态；若提供触发按钮，必须清楚标记 provider config requirement。
9. 文档明确 M7 不包含 streaming、tool execution、Node/Python process adapter、Tauri process management、embedding/document/OCR worker。
10. M7 文档明确 run-scoping 原则：worker-driven retrieval/recall 默认 run-scoped，global recall 必须显式 opt-in。

## 15. 后续 Milestone 8 handoff

M8 Tauri 桌面打包可以基于 M7：

- 管理 daemon 启停；
- 提供 provider config UI / secure storage；
- 管理本地 worker process lifecycle；
- 将 M7 的 provider worker 配置从环境变量迁移到桌面安全配置。

M7 不抢 M8 的桌面原生职责。
