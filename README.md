# Mneme

Mneme 是一个 Personal AI Kernel（个人 AI 内核）：面向本地优先、可审计、可扩展的 Agent 工作台，用来管理 runs、context、tools、memory、artifacts 和 operation history。

## 当前阶段

Phase 1 构建基础切片：

- Rust Kernel crate
- SQLite 持久化追加式 Operation Ledger
- 本地 Axum API
- daemon binary
- TypeScript API client
- 最小 React Workbench
- Artifact Workspace 与 Context Inspector 的只读基础面板

## 环境要求

- Rust stable via rustup
- Node.js 24+
- pnpm 11+

## 常用验证

```bash
pnpm check
git diff --check
```

## 启动本地 daemon

```bash
cargo run -p mneme-daemon
```

### 本地 SQLite 数据库

默认 daemon 使用 `sqlite://.mneme/mneme.db?mode=rwc`。如需隔离测试数据：

```bash
MNEME_DB_URL='sqlite://.mneme-dev/mneme.db?mode=rwc' cargo run -p mneme-daemon
```

Operation Ledger 的 `operation_events` 表是事实源；`runs`、`artifacts`、`context_bundles` 表是可重建投影。

## Kernel service boundary

HTTP API 不直接拼接 ledger 与 projection 写入流程。创建 run 会进入 Kernel `RunService`；daemon 默认使用 SQLite service，在同一事务内写入 `operation_events` 与 `runs`。

如需在启动时从 `operation_events` 修复 `runs` 读模型，可以显式开启：

```bash
MNEME_REBUILD_RUNS_ON_START=1 cargo run -p mneme-daemon
```

默认不自动 rebuild，避免每次启动都重写读模型；当迁移、调试或怀疑 projection 失配时再开启。

健康检查：

```bash
curl http://127.0.0.1:3766/api/v1/health
```

## 创建一条 run

```bash
curl -s -X POST http://127.0.0.1:3766/api/v1/runs \
  -H 'content-type: application/json' \
  -d '{"instruction":"整理今天的工作"}'

curl -s http://127.0.0.1:3766/api/v1/runs
```

### 手动推进 run lifecycle

Milestone 3 提供本地手动验证路径，用来确认 lifecycle event append、projection 和 Workbench 展示；这不代表已接入真实模型调用。

```bash
RUN_ID=$(curl -s http://127.0.0.1:3766/api/v1/runs \
  -H 'content-type: application/json' \
  -d '{"instruction":"整理今天的工作"}' | jq -r '.run_id')

curl -s "http://127.0.0.1:3766/api/v1/runs/$RUN_ID/model-requests" \
  -H 'content-type: application/json' \
  -d '{"text":"请总结今天的工作"}'

curl -s "http://127.0.0.1:3766/api/v1/runs/$RUN_ID/assistant-messages" \
  -H 'content-type: application/json' \
  -d '{"text":"今天完成了 Kernel service boundary。"}'

curl -s "http://127.0.0.1:3766/api/v1/runs/$RUN_ID/complete" \
  -H 'content-type: application/json' \
  -d '{"text":"总结完成"}'

curl -s http://127.0.0.1:3766/api/v1/runs
```

### 记录工具事件

Milestone 4 提供本地手动验证路径，用来确认 tool request、approval、result 事件会追加到 Operation Ledger，并更新 Workbench tool activity。启动 daemon 后可以用以下请求记录工具事件：

```bash
RUN_ID=$(curl -s http://127.0.0.1:3766/api/v1/runs \
  -H 'content-type: application/json' \
  -d '{"instruction":"读取 README"}' | jq -r '.run_id')

TOOL_CALL_ID=$(curl -s http://127.0.0.1:3766/api/v1/runs/$RUN_ID/tool-calls \
  -H 'content-type: application/json' \
  -d '{"tool_name":"read_file","effect":"read_only","input_json":{"path":"README.md"}}' | jq -r '.tool_call_id')

curl -s http://127.0.0.1:3766/api/v1/runs/$RUN_ID/tool-calls/$TOOL_CALL_ID/result \
  -H 'content-type: application/json' \
  -d '{"output_json":{"content":"示例内容"}}'
```

这些 endpoint 只记录工具事件，不执行真实文件读取。

### 创建 artifact 与 context bundle

Milestone 5 提供 artifact/context 的本地手动验证路径。Artifact 内容按 SHA-256 寻址，metadata 通过 `ArtifactCreated` 事件写入 Operation Ledger 并投影到 `artifacts`；context bundle 通过 `ContextBundleCreated` 事件记录 artifact/event 引用并投影到 `context_bundles`。

```bash
RUN_ID=$(curl -s http://127.0.0.1:3766/api/v1/runs \
  -H 'content-type: application/json' \
  -d '{"instruction":"整理 artifact context"}' | jq -r '.run_id')

ARTIFACT_ID=$(curl -s "http://127.0.0.1:3766/api/v1/runs/$RUN_ID/artifacts" \
  -H 'content-type: application/json' \
  -d '{"title":"Run summary","kind":"markdown","mime_type":"text/markdown","content":"# Summary\n\n完成了 Artifact Workspace。"}' | jq -r '.artifact_id')

curl -s "http://127.0.0.1:3766/api/v1/artifacts/$ARTIFACT_ID"

CONTEXT_BUNDLE_ID=$(curl -s "http://127.0.0.1:3766/api/v1/runs/$RUN_ID/context-bundles" \
  -H 'content-type: application/json' \
  -d "{\"title\":\"Debug context\",\"summary\":\"User report and generated artifact\",\"artifact_ids\":[\"$ARTIFACT_ID\"],\"event_ids\":[]}" | jq -r '.context_bundle_id')

curl -s "http://127.0.0.1:3766/api/v1/context-bundles/$CONTEXT_BUNDLE_ID"
curl -s http://127.0.0.1:3766/api/v1/context-bundles
```

这些 endpoint 只创建与读取已记录的 artifact/context 元数据和内容；不做搜索、向量召回、真实模型调用或真实工具执行。

## 启动基础阶段 Web shell

```bash
pnpm --filter @mneme/desktop dev
```
