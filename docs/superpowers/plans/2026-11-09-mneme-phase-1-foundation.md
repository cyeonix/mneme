# Mneme 第一阶段基础工程实施计划

> **给执行型 Agent 的要求：** 执行本计划时必须使用 `superpowers:subagent-driven-development`（推荐）或 `superpowers:executing-plans`。请逐个任务执行，并用复选框（`- [ ]`）跟踪进度。

**目标：** 建出 Mneme 第一个可测试的基础工程切片：Rust 工作区、Kernel 领域类型、内存版追加式 Operation Ledger、本地 Axum API、TypeScript API 客户端，以及一个能连到本地 Kernel 边界的最小 React Workbench。

**架构：** 本计划只实现已确认技术路线中最薄的一条纵向切片。Rust Kernel 负责领域模型、状态边界、API 权威入口；TypeScript UI 只通过 API client 访问 Kernel，不直接写状态。存储先使用内存版 Ledger 验证事件模型和 API 边界，SQLite 持久化放到下一阶段。

**技术栈：** Rust stable、Cargo workspace、Tokio、Axum、Serde、thiserror、uuid、time、tracing、Vite、React、TypeScript、Vitest、pnpm。

## 全局约束

- 产品方向：Mneme = Personal AI Kernel（个人 AI 内核）。
- 第一运行形态：Desktop App + Local Daemon/API。
- Kernel 路线：Rust-first Kernel + TypeScript UI。
- 接口路线：完整双接口：内部 typed RPC/event bus + 外部 HTTP/local API。
- 数据路线：Operation Ledger 作为事实源；SQLite 投影、搜索 sidecar、向量 sidecar 放到后续阶段。
- Runtime 路线：Rust Kernel 是任务、权限、账本、上下文、工具策略的权威控制面；模型执行通过可插拔 worker 完成。
- 工具路线：Mneme-native effect-typed Tool Protocol；MCP/OpenAPI/CLI/local-script 作为后续兼容适配层。
- 前端路线：React + Vite + Radix primitives + 自有设计系统。
- 同步路线：个人云服务器/自托管 Mneme Server 是后续里程碑；本阶段保持 API 边界可同步化。
- 当前环境可能还没有 Rust。若 `rustc --version` 失败，先安装 Rust stable，再开始任务 1。
- 每个任务都必须以测试/验证命令和 git commit 结束。

---

## 一、产品实施顺序总览

这个产品不能一次性做完，建议按下面里程碑推进。每个里程碑都应该有单独计划和验收关口。

### Milestone 0 — 仓库基础与第一条纵向切片

产出：干净 monorepo 骨架、Rust crates、daemon health/events API、TypeScript client、React Workbench。本文档就是这个里程碑的执行计划。

### Milestone 1 — 持久化 Operation Ledger 与投影

产出：SQLite 追加式事件表、SQLx migrations、runs/tasks/artifacts/tool calls 投影表、确定性 replay 测试。

### Milestone 2 — Kernel 服务边界

产出：AgentService、TaskService、RunService、ContextService、ToolService、PolicyService、EventService traits，以及经过测试的 command/query 流程。

### Milestone 3 — Agent run loop MVP

产出：创建 run、编译简单 ContextBundle、调用一个 OpenAI-compatible 或 Ollama provider、流式输出模型结果、记录完整 run events。

### Milestone 4 — Effect-typed tools 与审批

产出：ToolDescriptor、ToolCallIntent、PolicyService 审批决策、只读 NativeExecutor、UI 审批面板。

### Milestone 5 — Artifact 与 Context Inspector 工作台

产出：content-addressed artifact store、ContextBundle 可解释视图、Work Room、Operation Timeline、Context Inspector、Artifact Workspace。

### Milestone 6 — Search、Memory、Knowledge

产出：Tantivy 全文索引、向量索引抽象、memory metadata、knowledge sources、document chunks、ledger 中的 retrieval traces。

### Milestone 7 — Worker Runtime Protocol

产出：stdio JSON-RPC worker 协议、Rust-native provider worker、Node worker adapter、worker supervision、事件化 runtime errors。

### Milestone 8 — Tauri 桌面打包

产出：Tauri 2 shell，负责启动/管理本地 daemon、嵌入 React UI、通过薄 bridge 暴露原生 OS 能力。

### Milestone 9 — 自托管个人服务器

产出：设备注册、事件/对象同步、备份恢复、远程 Web console、基于 Ledger 的冲突规则。

---

## 二、本阶段目标文件结构

本阶段创建下面结构：

```text
mneme/
  Cargo.toml
  crates/
    mneme-kernel/
      Cargo.toml
      src/
        lib.rs
        health.rs
        ids.rs
        events.rs
        ledger.rs
    mneme-api/
      Cargo.toml
      src/
        lib.rs
        types.rs
        routes.rs
    mneme-daemon/
      Cargo.toml
      src/main.rs
  apps/
    desktop/
      package.json
      index.html
      tsconfig.json
      vite.config.ts
      src/
        main.tsx
        api/client.ts
        api/client.test.ts
        app/App.tsx
        app/App.test.tsx
        design-system/tokens.css
        styles.css
  package.json
  pnpm-workspace.yaml
  README.md
  .gitignore
  docs/
    architecture/phase-1-foundation.md
    superpowers/
      specs/2026-11-09-mneme-technical-selection-design.md
      plans/2026-11-09-mneme-phase-1-foundation.md
```

文件职责：

- `Cargo.toml`：Rust workspace 成员与共享依赖版本。
- `crates/mneme-kernel/src/health.rs`：Kernel 健康状态模型。
- `crates/mneme-kernel/src/ids.rs`：事件和 run 的强类型 ID。
- `crates/mneme-kernel/src/events.rs`：第一版 Operation Ledger 事件模型。
- `crates/mneme-kernel/src/ledger.rs`：追加式 Ledger trait 与内存实现。
- `crates/mneme-api/src/types.rs`：HTTP 外部 DTO，避免 UI 直接依赖内部领域类型细节。
- `crates/mneme-api/src/routes.rs`：Axum router 与 route handlers。
- `crates/mneme-daemon/src/main.rs`：本地 daemon 进程入口。
- `apps/desktop/src/api/client.ts`：前端访问 Kernel 的唯一 API client 边界。
- `apps/desktop/src/app/App.tsx`：最小 Mneme Workbench 页面。
- `apps/desktop/src/design-system/tokens.css`：第一版 Mneme 设计 token。

---

## Task 1：初始化 Rust workspace 与 Kernel 健康模型

**文件：**
- 创建：`Cargo.toml`
- 创建：`crates/mneme-kernel/Cargo.toml`
- 创建：`crates/mneme-kernel/src/lib.rs`
- 创建：`crates/mneme-kernel/src/health.rs`

**接口：**
- 消费：无。
- 产出：`mneme_kernel::KernelHealth`，`mneme_kernel::kernel_health() -> KernelHealth`。

- [ ] **Step 1：确认 Rust 可用**

运行：

```bash
rustc --version
cargo --version
```

期望：两个命令都打印版本并退出 0。若命令不存在，先安装 Rust stable：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

安装后打开新 shell，再重新运行版本检查。

- [ ] **Step 2：创建根 Rust workspace manifest**

写入 `Cargo.toml`：

```toml
[workspace]
members = [
  "crates/mneme-kernel",
]
resolver = "2"

[workspace.package]
edition = "2021"
license = "MIT"
repository = "https://github.com/cyeonix/mneme"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
time = { version = "0.3", features = ["serde", "formatting", "macros"] }
uuid = { version = "1.11", features = ["v4", "serde"] }
```

- [ ] **Step 3：创建 Kernel crate manifest**

写入 `crates/mneme-kernel/Cargo.toml`：

```toml
[package]
name = "mneme-kernel"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
serde.workspace = true
time.workspace = true
```

- [ ] **Step 4：创建 Kernel 对外模块文件**

写入 `crates/mneme-kernel/src/lib.rs`：

```rust
pub mod health;

pub use health::{kernel_health, KernelHealth};
```

- [ ] **Step 5：先写一个会失败的健康检查测试**

写入 `crates/mneme-kernel/src/health.rs`：

```rust
use serde::{Deserialize, Serialize};

/// Kernel 对外暴露的最小健康状态。
/// 后续可以继续增加 ledger 状态、worker 状态、数据库状态等字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelHealth {
    pub service: String,
    pub status: String,
    pub version: String,
}

pub fn kernel_health() -> KernelHealth {
    KernelHealth {
        service: "mneme-kernel".to_string(),
        // 这里先故意写成 starting，让测试先失败，确认测试真的在保护行为。
        status: "starting".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_health_reports_ready_service_identity() {
        let health = kernel_health();

        assert_eq!(health.service, "mneme-kernel");
        assert_eq!(health.status, "ready");
        assert_eq!(health.version, "0.1.0");
    }
}
```

- [ ] **Step 6：运行测试，确认它按预期失败**

```bash
cargo test -p mneme-kernel kernel_health_reports_ready_service_identity
```

期望：失败，并出现类似 left `"starting"`、right `"ready"` 的断言信息。

- [ ] **Step 7：实现最小修复**

把 `kernel_health()` 改成：

```rust
pub fn kernel_health() -> KernelHealth {
    KernelHealth {
        service: "mneme-kernel".to_string(),
        // 对外健康检查返回 ready，表示 Kernel 基础模块已经可以响应。
        status: "ready".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}
```

- [ ] **Step 8：运行测试，确认通过**

```bash
cargo test -p mneme-kernel kernel_health_reports_ready_service_identity
```

期望：PASS。

- [ ] **Step 9：运行格式检查和 Kernel 全量测试**

```bash
cargo fmt --all --check
cargo test -p mneme-kernel
```

期望：格式检查退出 0，Kernel crate 测试全部通过。

- [ ] **Step 10：提交**

```bash
git add Cargo.toml crates/mneme-kernel
git commit -m "feat: bootstrap Rust kernel crate"
```

---

## Task 2：添加强类型 ID 与 Operation Ledger 事件模型

**文件：**
- 修改：`crates/mneme-kernel/src/lib.rs`
- 修改：`crates/mneme-kernel/Cargo.toml`
- 创建：`crates/mneme-kernel/src/ids.rs`
- 创建：`crates/mneme-kernel/src/events.rs`

**接口：**
- 消费：Task 1 的 `KernelHealth`。
- 产出：`EventId::new() -> EventId`，`RunId::new() -> RunId`，`OperationEvent`，`OperationEventKind`，`OperationEvent::new_run_started(run_id: RunId, instruction: impl Into<String>) -> OperationEvent`。

- [ ] **Step 1：给 Kernel crate 加 uuid/time 依赖**

更新 `crates/mneme-kernel/Cargo.toml` 的 `[dependencies]`：

```toml
[dependencies]
serde.workspace = true
time.workspace = true
uuid.workspace = true
```

- [ ] **Step 2：创建强类型 ID 模块和测试**

写入 `crates/mneme-kernel/src/ids.rs`：

```rust
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Ledger 中每条事件的唯一 ID。
/// 用 newtype 包一层，避免把 EventId 和 RunId 误传。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(Uuid);

/// 一次 Agent/用户工作流运行的唯一 ID。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(Uuid);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl RunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_event_ids_are_unique() {
        let first = EventId::new();
        let second = EventId::new();

        assert_ne!(first, second);
    }

    #[test]
    fn generated_run_ids_are_displayable() {
        let run_id = RunId::new().to_string();

        assert_eq!(run_id.len(), 36);
        assert!(run_id.contains('-'));
    }
}
```

- [ ] **Step 3：创建 OperationEvent 事件模型和测试**

写入 `crates/mneme-kernel/src/events.rs`：

```rust
use crate::ids::{EventId, RunId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// 第一版事件类型。
/// 先覆盖 run 生命周期，后续再扩展 tool、artifact、context、memory 等事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationEventKind {
    RunStarted { instruction: String },
    RunCompleted { summary: String },
    RunFailed { reason: String },
}

/// Operation Ledger 的最小事件结构。
/// 事件一旦写入就不应该原地修改，后续状态通过追加新事件表达。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationEvent {
    pub id: EventId,
    pub run_id: RunId,
    pub occurred_at: OffsetDateTime,
    pub kind: OperationEventKind,
}

impl OperationEvent {
    pub fn new_run_started(run_id: RunId, instruction: impl Into<String>) -> Self {
        Self {
            id: EventId::new(),
            run_id,
            occurred_at: OffsetDateTime::now_utc(),
            kind: OperationEventKind::RunStarted {
                instruction: instruction.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_started_event_captures_instruction_and_run_id() {
        let run_id = RunId::new();
        let event = OperationEvent::new_run_started(run_id, "Draft the Mneme roadmap");

        assert_eq!(event.run_id, run_id);
        assert!(matches!(
            event.kind,
            OperationEventKind::RunStarted { ref instruction }
                if instruction == "Draft the Mneme roadmap"
        ));
    }
}
```

- [ ] **Step 4：导出新模块**

更新 `crates/mneme-kernel/src/lib.rs`：

```rust
pub mod events;
pub mod health;
pub mod ids;

pub use events::{OperationEvent, OperationEventKind};
pub use health::{kernel_health, KernelHealth};
pub use ids::{EventId, RunId};
```

- [ ] **Step 5：运行测试**

```bash
cargo fmt --all --check
cargo test -p mneme-kernel
```

期望：health、IDs、events 测试全部通过。

- [ ] **Step 6：提交**

```bash
git add crates/mneme-kernel/Cargo.toml crates/mneme-kernel/src
git commit -m "feat: add kernel operation event model"
```

---

## Task 3：添加追加式内存 Operation Ledger

**文件：**
- 修改：`crates/mneme-kernel/src/lib.rs`
- 修改：`crates/mneme-kernel/Cargo.toml`
- 创建：`crates/mneme-kernel/src/ledger.rs`

**接口：**
- 消费：Task 2 的 `OperationEvent`、`RunId`。
- 产出：`OperationLedger` trait、`InMemoryOperationLedger`、`LedgerError`。

- [ ] **Step 1：添加 thiserror 依赖**

更新 `crates/mneme-kernel/Cargo.toml`：

```toml
[dependencies]
serde.workspace = true
thiserror.workspace = true
time.workspace = true
uuid.workspace = true
```

- [ ] **Step 2：写 Ledger 模块和 append/list 测试**

写入 `crates/mneme-kernel/src/ledger.rs`：

```rust
use crate::{OperationEvent, RunId};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LedgerError {
    #[error("operation ledger lock is poisoned")]
    Poisoned,
}

/// Operation Ledger 的抽象边界。
/// 后续 SQLite 实现也应该实现这个 trait，让 API 层不关心具体存储。
pub trait OperationLedger: Send + Sync {
    fn append(&self, event: OperationEvent) -> Result<(), LedgerError>;
    fn list(&self) -> Result<Vec<OperationEvent>, LedgerError>;
    fn list_for_run(&self, run_id: RunId) -> Result<Vec<OperationEvent>, LedgerError>;
}

/// 第一阶段只使用内存版 Ledger，用来验证领域模型和 API 边界。
#[derive(Debug, Clone, Default)]
pub struct InMemoryOperationLedger {
    events: Arc<Mutex<Vec<OperationEvent>>>,
}

impl InMemoryOperationLedger {
    pub fn new() -> Self {
        Self::default()
    }
}

impl OperationLedger for InMemoryOperationLedger {
    fn append(&self, event: OperationEvent) -> Result<(), LedgerError> {
        let mut events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        events.push(event);
        Ok(())
    }

    fn list(&self) -> Result<Vec<OperationEvent>, LedgerError> {
        let events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        Ok(events.clone())
    }

    fn list_for_run(&self, run_id: RunId) -> Result<Vec<OperationEvent>, LedgerError> {
        let events = self.events.lock().map_err(|_| LedgerError::Poisoned)?;
        Ok(events
            .iter()
            .filter(|event| event.run_id == run_id)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OperationEvent;

    #[test]
    fn ledger_appends_events_in_order() {
        let ledger = InMemoryOperationLedger::new();
        let run_id = RunId::new();
        let first = OperationEvent::new_run_started(run_id, "First instruction");
        let second = OperationEvent::new_run_started(run_id, "Second instruction");

        ledger.append(first.clone()).expect("append first event");
        ledger.append(second.clone()).expect("append second event");

        assert_eq!(ledger.list().expect("list events"), vec![first, second]);
    }

    #[test]
    fn ledger_lists_events_for_one_run() {
        let ledger = InMemoryOperationLedger::new();
        let selected_run = RunId::new();
        let other_run = RunId::new();
        let selected_event = OperationEvent::new_run_started(selected_run, "Selected run");
        let other_event = OperationEvent::new_run_started(other_run, "Other run");

        ledger.append(selected_event.clone()).expect("append selected event");
        ledger.append(other_event).expect("append other event");

        assert_eq!(
            ledger.list_for_run(selected_run).expect("list selected run"),
            vec![selected_event]
        );
    }
}
```

- [ ] **Step 3：导出 Ledger 接口**

更新 `crates/mneme-kernel/src/lib.rs`：

```rust
pub mod events;
pub mod health;
pub mod ids;
pub mod ledger;

pub use events::{OperationEvent, OperationEventKind};
pub use health::{kernel_health, KernelHealth};
pub use ids::{EventId, RunId};
pub use ledger::{InMemoryOperationLedger, LedgerError, OperationLedger};
```

- [ ] **Step 4：运行测试**

```bash
cargo fmt --all --check
cargo test -p mneme-kernel
```

期望：Kernel 测试全部通过。

- [ ] **Step 5：提交**

```bash
git add crates/mneme-kernel/Cargo.toml crates/mneme-kernel/src
git commit -m "feat: add in-memory operation ledger"
```

---

## Task 4：添加本地 HTTP API crate

**文件：**
- 修改：`Cargo.toml`
- 创建：`crates/mneme-api/Cargo.toml`
- 创建：`crates/mneme-api/src/lib.rs`
- 创建：`crates/mneme-api/src/types.rs`
- 创建：`crates/mneme-api/src/routes.rs`

**接口：**
- 消费：`kernel_health()`、`InMemoryOperationLedger`、`OperationLedger`、`OperationEvent`、`RunId`。
- 产出：`mneme_api::build_router(ledger: InMemoryOperationLedger) -> axum::Router`，以及 HTTP `GET /api/v1/health`、`GET /api/v1/events`、`POST /api/v1/runs`。

- [ ] **Step 1：把 API crate 加入 workspace**

更新根 `Cargo.toml`：

```toml
[workspace]
members = [
  "crates/mneme-kernel",
  "crates/mneme-api",
]
resolver = "2"

[workspace.dependencies]
axum = "0.7"
http-body-util = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
time = { version = "0.3", features = ["serde", "formatting", "macros"] }
tokio = { version = "1.41", features = ["macros", "rt-multi-thread", "net"] }
tower = "0.5"
uuid = { version = "1.11", features = ["v4", "serde"] }
```

- [ ] **Step 2：创建 API crate manifest**

写入 `crates/mneme-api/Cargo.toml`：

```toml
[package]
name = "mneme-api"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
axum.workspace = true
mneme-kernel = { path = "../mneme-kernel" }
serde.workspace = true
serde_json.workspace = true
time.workspace = true

[dev-dependencies]
http-body-util.workspace = true
tokio.workspace = true
tower.workspace = true
```

- [ ] **Step 3：创建 API DTO**

写入 `crates/mneme-api/src/types.rs`：

```rust
use mneme_kernel::{KernelHealth, OperationEvent};
use serde::{Deserialize, Serialize};

/// HTTP health 返回体。API DTO 与 Kernel 领域类型分开，便于以后做版本兼容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthResponse {
    pub service: String,
    pub status: String,
    pub version: String,
}

impl From<KernelHealth> for HealthResponse {
    fn from(value: KernelHealth) -> Self {
        Self {
            service: value.service,
            status: value.status,
            version: value.version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRunRequest {
    pub instruction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRunResponse {
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventsResponse {
    pub events: Vec<OperationEvent>,
}
```

- [ ] **Step 4：创建 routes 与 route 测试**

写入 `crates/mneme-api/src/routes.rs`：

```rust
use crate::types::{CreateRunRequest, CreateRunResponse, EventsResponse, HealthResponse};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use mneme_kernel::{kernel_health, InMemoryOperationLedger, OperationEvent, OperationLedger, RunId};

#[derive(Clone)]
pub struct ApiState {
    pub ledger: InMemoryOperationLedger,
}

/// 创建第一版本地 API 路由。
/// 这里显式传入 ledger，便于测试中复用同一个内存账本检查副作用。
pub fn build_router(ledger: InMemoryOperationLedger) -> Router {
    Router::new()
        .route("/api/v1/health", get(get_health))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/runs", post(create_run))
        .with_state(ApiState { ledger })
}

async fn get_health() -> Json<HealthResponse> {
    Json(kernel_health().into())
}

async fn list_events(State(state): State<ApiState>) -> Result<Json<EventsResponse>, StatusCode> {
    let events = state
        .ledger
        .list()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(EventsResponse { events }))
}

async fn create_run(
    State(state): State<ApiState>,
    Json(request): Json<CreateRunRequest>,
) -> Result<Json<CreateRunResponse>, StatusCode> {
    if request.instruction.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let run_id = RunId::new();
    let event = OperationEvent::new_run_started(run_id, request.instruction);
    state
        .ledger
        .append(event)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(CreateRunResponse {
        run_id: run_id.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_endpoint_returns_ready_kernel_status() {
        let app = build_router(InMemoryOperationLedger::new());

        let response = app
            .oneshot(Request::builder().uri("/api/v1/health").body(Body::empty()).expect("request"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.expect("body").to_bytes();
        let payload: HealthResponse = serde_json::from_slice(&body).expect("health response");

        assert_eq!(payload.service, "mneme-kernel");
        assert_eq!(payload.status, "ready");
    }

    #[tokio::test]
    async fn create_run_appends_run_started_event() {
        let ledger = InMemoryOperationLedger::new();
        let app = build_router(ledger.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "instruction": "Build the first Mneme run" }).to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(ledger.list().expect("events").len(), 1);
    }

    #[tokio::test]
    async fn create_run_rejects_blank_instruction() {
        let app = build_router(InMemoryOperationLedger::new());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/runs")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "instruction": "   " }).to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
```

- [ ] **Step 5：导出 API 模块**

写入 `crates/mneme-api/src/lib.rs`：

```rust
pub mod routes;
pub mod types;

pub use routes::build_router;
```

- [ ] **Step 6：运行 API 测试**

```bash
cargo fmt --all --check
cargo test -p mneme-api
cargo test --workspace
```

期望：API route 测试与 Kernel 测试全部通过。

- [ ] **Step 7：提交**

```bash
git add Cargo.toml crates/mneme-api
git commit -m "feat: add local API routes"
```

---

## Task 5：添加本地 daemon binary

**文件：**
- 修改：`Cargo.toml`
- 创建：`crates/mneme-daemon/Cargo.toml`
- 创建：`crates/mneme-daemon/src/main.rs`

**接口：**
- 消费：`mneme_api::build_router`、`mneme_kernel::InMemoryOperationLedger`。
- 产出：`mneme-daemon` binary，默认监听 `127.0.0.1:3766`。

- [ ] **Step 1：把 daemon crate 加入 workspace**

更新根 `Cargo.toml` 的 workspace members，并添加 tracing 依赖：

```toml
[workspace]
members = [
  "crates/mneme-kernel",
  "crates/mneme-api",
  "crates/mneme-daemon",
]
resolver = "2"

[workspace.dependencies]
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

保留前面已经添加的其他 workspace dependencies。

- [ ] **Step 2：创建 daemon crate manifest**

写入 `crates/mneme-daemon/Cargo.toml`：

```toml
[package]
name = "mneme-daemon"
version = "0.1.0"
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
axum.workspace = true
mneme-api = { path = "../mneme-api" }
mneme-kernel = { path = "../mneme-kernel" }
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
```

- [ ] **Step 3：创建 daemon main**

写入 `crates/mneme-daemon/src/main.rs`：

```rust
use mneme_api::build_router;
use mneme_kernel::InMemoryOperationLedger;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mneme_daemon=info,tower_http=info".into()),
        )
        .init();

    // 默认只绑定本机地址，避免第一阶段误暴露到局域网。
    let addr: SocketAddr = std::env::var("MNEME_BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3766".to_string())
        .parse()?;

    // 第一阶段使用内存账本；持久化 SQLite 会在下一阶段替换这里。
    let ledger = InMemoryOperationLedger::new();
    let app = build_router(ledger);
    let listener = TcpListener::bind(addr).await?;

    info!(%addr, "mneme daemon listening");
    axum::serve(listener, app).await?;

    Ok(())
}
```

- [ ] **Step 4：运行 daemon 检查**

```bash
cargo fmt --all --check
cargo check -p mneme-daemon
cargo test --workspace
```

期望：daemon 可编译，workspace 测试全部通过。

- [ ] **Step 5：手动验证 health endpoint**

一个终端运行：

```bash
cargo run -p mneme-daemon
```

另一个终端运行：

```bash
curl http://127.0.0.1:3766/api/v1/health
```

期望返回 JSON 包含：

```json
{"service":"mneme-kernel","status":"ready","version":"0.1.0"}
```

验证后用 `Ctrl+C` 停止 daemon。

- [ ] **Step 6：提交**

```bash
git add Cargo.toml crates/mneme-daemon
git commit -m "feat: add local daemon binary"
```

---

## Task 6：初始化 pnpm workspace 与 TypeScript API client

**文件：**
- 创建：`package.json`
- 创建：`pnpm-workspace.yaml`
- 创建：`apps/desktop/package.json`
- 创建：`apps/desktop/tsconfig.json`
- 创建：`apps/desktop/vite.config.ts`
- 创建：`apps/desktop/src/api/client.ts`
- 创建：`apps/desktop/src/api/client.test.ts`

**接口：**
- 消费：Task 4/5 的 HTTP API。
- 产出：`MnemeApiClient`、`createMnemeApiClient(baseUrl: string): MnemeApiClient`、`KernelHealth`、`OperationEvent`、`CreateRunResponse`。

- [ ] **Step 1：创建根 pnpm workspace 文件**

写入 `package.json`：

```json
{
  "name": "mneme",
  "private": true,
  "version": "0.1.0",
  "packageManager": "pnpm@11.9.0",
  "scripts": {
    "check": "pnpm --recursive check",
    "test": "pnpm --recursive test"
  }
}
```

写入 `pnpm-workspace.yaml`：

```yaml
packages:
  - apps/*
  - packages/*
```

- [ ] **Step 2：创建 desktop package manifest**

写入 `apps/desktop/package.json`：

```json
{
  "name": "@mneme/desktop",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "check": "tsc --noEmit",
    "test": "vitest run",
    "dev": "vite",
    "build": "tsc --noEmit && vite build"
  },
  "dependencies": {
    "@vitejs/plugin-react": "latest",
    "vite": "latest",
    "typescript": "latest",
    "react": "latest",
    "react-dom": "latest"
  },
  "devDependencies": {
    "vitest": "latest",
    "jsdom": "latest",
    "@testing-library/react": "latest",
    "@testing-library/jest-dom": "latest",
    "@types/react": "latest",
    "@types/react-dom": "latest"
  }
}
```

- [ ] **Step 3：创建 TypeScript 与 Vite 配置**

写入 `apps/desktop/tsconfig.json` 和 `apps/desktop/vite.config.ts`。Vite 配置如下：

```ts
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
  },
});
```

`tsconfig.json` 使用严格模式：`strict: true`、`noEmit: true`、`jsx: "react-jsx"`。

- [ ] **Step 4：先写 API client 测试**

写入 `apps/desktop/src/api/client.test.ts`：

```ts
import { describe, expect, it, vi } from 'vitest';
import { createMnemeApiClient } from './client';

describe('createMnemeApiClient', () => {
  it('从配置的 base URL 读取 Kernel 健康状态', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ service: 'mneme-kernel', status: 'ready', version: '0.1.0' }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const client = createMnemeApiClient('http://127.0.0.1:3766', fetchMock);

    const health = await client.getHealth();

    expect(fetchMock).toHaveBeenCalledWith('http://127.0.0.1:3766/api/v1/health', {
      headers: { accept: 'application/json' },
    });
    expect(health.status).toBe('ready');
  });

  it('提交 run instruction 并返回新 run id', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ run_id: 'run-123' }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    );
    const client = createMnemeApiClient('http://127.0.0.1:3766', fetchMock);

    const response = await client.createRun('Draft the first Mneme task');

    expect(fetchMock).toHaveBeenCalledWith('http://127.0.0.1:3766/api/v1/runs', {
      method: 'POST',
      headers: { accept: 'application/json', 'content-type': 'application/json' },
      body: JSON.stringify({ instruction: 'Draft the first Mneme task' }),
    });
    expect(response.run_id).toBe('run-123');
  });
});
```

- [ ] **Step 5：运行测试确认失败**

```bash
pnpm install
pnpm --filter @mneme/desktop test -- src/api/client.test.ts
```

期望：失败，因为 `./client` 还不存在。

- [ ] **Step 6：实现 API client**

写入 `apps/desktop/src/api/client.ts`：

```ts
export interface KernelHealth {
  service: string;
  status: string;
  version: string;
}

export interface OperationEvent {
  id: string;
  run_id: string;
  occurred_at: string;
  kind: Record<string, unknown>;
}

export interface EventsResponse {
  events: OperationEvent[];
}

export interface CreateRunResponse {
  run_id: string;
}

export interface MnemeApiClient {
  getHealth(): Promise<KernelHealth>;
  listEvents(): Promise<EventsResponse>;
  createRun(instruction: string): Promise<CreateRunResponse>;
}

type FetchLike = typeof fetch;

async function parseJsonResponse<T>(response: Response): Promise<T> {
  if (!response.ok) {
    throw new Error(`Mneme API request failed with status ${response.status}`);
  }
  return (await response.json()) as T;
}

export function createMnemeApiClient(baseUrl: string, fetchImpl: FetchLike = fetch): MnemeApiClient {
  // 统一去掉结尾斜杠，避免拼接出双斜杠 URL。
  const normalizedBaseUrl = baseUrl.replace(/\/$/, '');

  return {
    async getHealth() {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/health`, {
        headers: { accept: 'application/json' },
      });
      return parseJsonResponse<KernelHealth>(response);
    },

    async listEvents() {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/events`, {
        headers: { accept: 'application/json' },
      });
      return parseJsonResponse<EventsResponse>(response);
    },

    async createRun(instruction: string) {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/runs`, {
        method: 'POST',
        headers: { accept: 'application/json', 'content-type': 'application/json' },
        body: JSON.stringify({ instruction }),
      });
      return parseJsonResponse<CreateRunResponse>(response);
    },
  };
}
```

- [ ] **Step 7：运行 client 测试和类型检查**

```bash
pnpm --filter @mneme/desktop test -- src/api/client.test.ts
pnpm --filter @mneme/desktop check
```

期望：测试通过，TypeScript check 退出 0。

- [ ] **Step 8：提交**

```bash
git add package.json pnpm-workspace.yaml pnpm-lock.yaml apps/desktop
git commit -m "feat: add TypeScript API client"
```

---

## Task 7：添加最小 React Workbench

**文件：**
- 创建：`apps/desktop/index.html`
- 创建：`apps/desktop/src/main.tsx`
- 创建：`apps/desktop/src/app/App.tsx`
- 创建：`apps/desktop/src/app/App.test.tsx`
- 创建：`apps/desktop/src/design-system/tokens.css`
- 创建：`apps/desktop/src/styles.css`

**接口：**
- 消费：Task 6 的 `MnemeApiClient`、`KernelHealth`、`EventsResponse`。
- 产出：`App({ client }: { client: MnemeApiClient }): JSX.Element`。

- [ ] **Step 1：先写组件测试**

写入 `apps/desktop/src/app/App.test.tsx`：

```tsx
import '@testing-library/jest-dom/vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { MnemeApiClient } from '../api/client';
import { App } from './App';

function createClient(): MnemeApiClient {
  return {
    async getHealth() {
      return { service: 'mneme-kernel', status: 'ready', version: '0.1.0' };
    },
    async listEvents() {
      return { events: [] };
    },
    async createRun() {
      return { run_id: 'run-123' };
    },
  };
}

describe('App', () => {
  it('渲染 Mneme Workbench 和 Kernel 健康状态', async () => {
    render(<App client={createClient()} />);

    expect(screen.getByRole('heading', { name: 'Mneme Workbench' })).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByText('Kernel ready')).toBeInTheDocument();
    });
  });
});
```

- [ ] **Step 2：运行测试确认失败**

```bash
pnpm --filter @mneme/desktop test -- src/app/App.test.tsx
```

期望：失败，因为 `./App` 还不存在。

- [ ] **Step 3：实现 App 组件**

写入 `apps/desktop/src/app/App.tsx`：

```tsx
import { useEffect, useState } from 'react';
import type { EventsResponse, KernelHealth, MnemeApiClient } from '../api/client';

interface AppProps {
  client: MnemeApiClient;
}

export function App({ client }: AppProps) {
  const [health, setHealth] = useState<KernelHealth | null>(null);
  const [events, setEvents] = useState<EventsResponse>({ events: [] });
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadKernelState() {
      try {
        // 第一版页面只读取健康状态和事件列表，证明 UI 已经通过 API 边界访问 Kernel。
        const [nextHealth, nextEvents] = await Promise.all([
          client.getHealth(),
          client.listEvents(),
        ]);
        if (!cancelled) {
          setHealth(nextHealth);
          setEvents(nextEvents);
        }
      } catch (unknownError) {
        if (!cancelled) {
          setError(unknownError instanceof Error ? unknownError.message : 'Unknown Mneme API error');
        }
      }
    }

    void loadKernelState();

    return () => {
      cancelled = true;
    };
  }, [client]);

  return (
    <main className="mneme-shell">
      <section className="hero-panel">
        <p className="eyebrow">Personal AI Kernel</p>
        <h1>Mneme Workbench</h1>
        <p className="lede">A local-first control room for runs, context, tools, artifacts, and memory.</p>
      </section>

      <section className="status-grid" aria-label="Kernel status">
        <article className="status-card">
          <span className="label">Kernel</span>
          <strong>{health ? `Kernel ${health.status}` : 'Connecting to kernel'}</strong>
          <span className="meta">{health ? `${health.service} ${health.version}` : 'Waiting for local daemon'}</span>
        </article>
        <article className="status-card">
          <span className="label">Operation Ledger</span>
          <strong>{events.events.length} events</strong>
          <span className="meta">Append-only event stream foundation</span>
        </article>
      </section>

      {error ? <p className="error-banner">{error}</p> : null}
    </main>
  );
}
```

- [ ] **Step 4：创建浏览器入口和样式**

写入 `apps/desktop/index.html`、`apps/desktop/src/main.tsx`、`apps/desktop/src/design-system/tokens.css`、`apps/desktop/src/styles.css`。`main.tsx`：

```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import { createMnemeApiClient } from './api/client';
import { App } from './app/App';
import './design-system/tokens.css';
import './styles.css';

// 开发期默认连接本地 daemon；后续 Tauri 阶段可以改成由桌面壳注入。
const client = createMnemeApiClient(import.meta.env.VITE_MNEME_API_BASE_URL ?? 'http://127.0.0.1:3766');

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App client={client} />
  </React.StrictMode>,
);
```

`tokens.css` 先定义深色主题、间距、圆角、字体 token；`styles.css` 创建 hero panel、status grid、error banner、移动端单列布局。

- [ ] **Step 5：运行 app 测试和构建**

```bash
pnpm --filter @mneme/desktop test -- src/app/App.test.tsx
pnpm --filter @mneme/desktop check
pnpm --filter @mneme/desktop build
```

期望：组件测试通过，TypeScript check 退出 0，Vite build 成功。

- [ ] **Step 6：提交**

```bash
git add apps/desktop
git commit -m "feat: add minimal Mneme workbench"
```

---

## Task 8：添加根验证脚本与基础文档

**文件：**
- 修改：`package.json`
- 创建：`.gitignore`
- 创建：`README.md`
- 创建：`docs/architecture/phase-1-foundation.md`

**接口：**
- 消费：前面任务创建的 Rust workspace 与 desktop workspace。
- 产出：给后续开发者和 Agent 使用的统一验证命令与基础架构说明。

- [ ] **Step 1：添加根验证脚本**

更新根 `package.json`：

```json
{
  "name": "mneme",
  "private": true,
  "version": "0.1.0",
  "packageManager": "pnpm@11.9.0",
  "scripts": {
    "check": "pnpm check:rust && pnpm check:ui",
    "check:rust": "cargo fmt --all --check && cargo test --workspace",
    "check:ui": "pnpm --recursive check && pnpm --recursive test",
    "test": "pnpm test:rust && pnpm test:ui",
    "test:rust": "cargo test --workspace",
    "test:ui": "pnpm --recursive test"
  }
}
```

- [ ] **Step 2：创建 gitignore**

写入 `.gitignore`：

```gitignore
/target/
/node_modules/
/apps/*/dist/
/.DS_Store
*.log
.env
.env.*
!.env.example
```

- [ ] **Step 3：创建中文 README**

写入 `README.md`：

````markdown
# Mneme

Mneme 是一个 Personal AI Kernel（个人 AI 内核）：面向本地优先、可审计、可扩展的 Agent 工作台，用来管理 runs、context、tools、memory、artifacts 和 operation history。

## 当前阶段

Phase 1 构建基础切片：

- Rust Kernel crate
- 内存版追加式 Operation Ledger
- 本地 Axum API
- daemon binary
- TypeScript API client
- 最小 React Workbench

## 环境要求

- Rust stable via rustup
- Node.js 24+
- pnpm 11+

## 验证

```bash
pnpm check
```

## 启动本地 daemon

```bash
cargo run -p mneme-daemon
```

健康检查：

```bash
curl http://127.0.0.1:3766/api/v1/health
```

## 启动基础阶段 Web shell

```bash
pnpm --filter @mneme/desktop dev
```
````

- [ ] **Step 4：创建架构说明**

写入 `docs/architecture/phase-1-foundation.md`：

````markdown
# Phase 1 Foundation Architecture

Phase 1 只验证 Mneme 的第一条纵向切片，不包含持久化存储和真实模型调用。

## 边界

- `mneme-kernel` 拥有领域类型和 Operation Ledger 接口。
- `mneme-api` 暴露 HTTP DTO 与 routes。
- `mneme-daemon` 负责进程启动和本地监听地址。
- `apps/desktop` 通过 typed API client 消费 Kernel API，不直接写 Kernel 状态。

## 数据流

```text
React Workbench
  → MnemeApiClient
  → HTTP /api/v1/*
  → mneme-api routes
  → mneme-kernel ledger
```

## 当前存储

本阶段 Ledger 是内存实现。下一阶段再加入 SQLite 持久化，这样可以先确认事件 schema 与 API 边界足够稳定。

## 验证

```bash
pnpm check
```
````

- [ ] **Step 5：运行完整验证**

```bash
pnpm check
```

期望：Rust 格式检查与测试通过；TypeScript check 与 Vitest 测试通过。

- [ ] **Step 6：提交**

```bash
git add .gitignore README.md package.json docs/architecture/phase-1-foundation.md
git commit -m "docs: document phase 1 foundation"
```

---

## 三、本计划最终验证

完成 Task 8 后运行：

```bash
git status --short --branch
pnpm check
git log --oneline -8
```

期望：

- `git status --short --branch` 显示分支干净，或只显示明确准备 push 的本地 commit。
- `pnpm check` 退出 0。
- `git log --oneline -8` 能看到本计划每个任务对应的 commit。

最终 push：

```bash
git push origin main
```

---

## 四、本阶段明确不做的事情

这些内容留到后续里程碑，不要混进 Phase 1：

- SQLite 持久化与 SQLx migrations。
- Tauri 2 桌面打包。
- 生成 OpenAPI / 生成 TypeScript SDK。
- 真实 LLM provider 调用。
- worker protocol 实现。
- effect-typed tool 执行。
- MCP/OpenAPI/CLI adapters。
- content-addressed artifact store。
- Tantivy 或 vector indexes。
- self-hosted server。
- authentication 或 device registration。
