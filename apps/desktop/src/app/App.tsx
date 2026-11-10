import { useEffect, useState } from 'react';
import type { EventsResponse, KernelHealth, MnemeApiClient, RunsResponse } from '../api/client';

interface AppProps {
  client: MnemeApiClient;
}

interface ToolActivityItem {
  eventId: string;
  runId: string;
  occurredAt: string;
  toolCallId: string;
  label: string;
  detail: string;
  effect?: string;
}

function toolActivityFromEvents(events: EventsResponse): ToolActivityItem[] {
  return events.events.flatMap((event) => {
    const kind = event.kind as Record<string, unknown>;
    if (kind.type === 'tool_call_requested') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: String(kind.tool_name),
          detail: 'requested',
          effect: String(kind.effect),
        },
      ];
    }
    if (kind.type === 'tool_call_approved') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: 'approved',
          detail: String(kind.approved_by),
        },
      ];
    }
    if (kind.type === 'tool_call_denied' || kind.type === 'tool_call_failed') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: String(kind.type),
          detail: String(kind.reason),
        },
      ];
    }
    if (kind.type === 'tool_call_completed') {
      return [
        {
          eventId: event.id,
          runId: event.run_id,
          occurredAt: event.occurred_at,
          toolCallId: String(kind.tool_call_id),
          label: 'completed',
          detail: 'result recorded',
        },
      ];
    }
    return [];
  });
}

export function App({ client }: AppProps) {
  const [health, setHealth] = useState<KernelHealth | null>(null);
  const [events, setEvents] = useState<EventsResponse>({ events: [] });
  const [runs, setRuns] = useState<RunsResponse>({ runs: [] });
  const [error, setError] = useState<string | null>(null);
  const toolActivity = toolActivityFromEvents(events);

  useEffect(() => {
    let cancelled = false;

    async function loadKernelState() {
      try {
        // Workbench 通过 API 边界读取 Kernel 状态、Operation Ledger 事件和 runs 读模型。
        const [nextHealth, nextEvents, nextRuns] = await Promise.all([
          client.getHealth(),
          client.listEvents(),
          client.listRuns(),
        ]);
        if (!cancelled) {
          setHealth(nextHealth);
          setEvents(nextEvents);
          setRuns(nextRuns);
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

      <section className="panel">
        <h2>Runs</h2>
        {runs.runs.length === 0 ? (
          <p className="muted">还没有持久化 run。通过 API 创建 run 后会显示在这里。</p>
        ) : (
          <ul className="run-list">
            {runs.runs.map((run) => (
              <li key={run.run_id} className="run-card">
                <span className="run-status">{run.status}</span>
                <strong>{run.instruction}</strong>
                <small>{run.updated_at}</small>
                {run.summary ? (
                  <p className="run-result">
                    总结：<span>{run.summary}</span>
                  </p>
                ) : null}
                {run.error ? (
                  <p className="run-error">
                    错误：<span>{run.error}</span>
                  </p>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="panel">
        <h2>Tool Activity</h2>
        {toolActivity.length === 0 ? (
          <p className="muted">还没有工具请求。Milestone 4 只展示工具意图和审批记录，不执行真实工具。</p>
        ) : (
          <ul className="tool-activity-list">
            {toolActivity.map((activity) => (
              <li key={activity.eventId} className="tool-activity-card">
                <span className="tool-activity-kind">{activity.label}</span>
                {activity.effect ? <span className="tool-effect">{activity.effect}</span> : null}
                <strong>{activity.detail}</strong>
                <small>{activity.occurredAt}</small>
              </li>
            ))}
          </ul>
        )}
      </section>

      {error ? <p className="error-banner">{error}</p> : null}
    </main>
  );
}
