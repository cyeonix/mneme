# Milestone 3: Agent Run Loop MVP

Milestone 3 为 Mneme 增加无工具、无真实模型调用的 run 生命周期骨架。它把 run 从单个 `RunStarted` 事件扩展为可追加的事件流：`RunStarted → RunModelRequested → RunAssistantMessage → RunCompleted/RunFailed`。

## 边界

本里程碑实现生命周期记录、查询、投影和 Workbench 展示；不接入真实 LLM，不实现 streaming，不实现工具调用或审批，不实现 worker runtime。

## 事件流

- `RunStarted { instruction }`：用户创建 run。
- `RunModelRequested { prompt }`：Kernel 记录准备请求模型的 prompt。
- `RunAssistantMessage { content }`：Kernel 记录助手回复内容。
- `RunCompleted { summary }`：run 成功结束。
- `RunFailed { reason }`：run 失败结束。

所有事件写入 `operation_events`，`runs` 表只是可重建读模型。

## Service Boundary

HTTP API、daemon 和未来 worker 都必须调用 `RunService`。Milestone 3 新增 lifecycle 方法，让后续真实 Agent loop 不需要绕过 Kernel 业务边界。

## HTTP 验证

```bash
cargo run -p mneme-daemon
```

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

## 后续衔接

Milestone 4 可以把 tool request/approval 作为新的事件类型追加到同一个 ledger；Milestone 7 可以让 worker runtime 调用同一组 `RunService` lifecycle 方法，而不是直接写 SQLite。
