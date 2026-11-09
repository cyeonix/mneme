import { useEffect, useState } from 'react';
import type { EventsResponse, KernelHealth, MnemeApiClient, RunsResponse } from '../api/client';

interface AppProps {
  client: MnemeApiClient;
}

export function App({ client }: AppProps) {
  const [health, setHealth] = useState<KernelHealth | null>(null);
  const [events, setEvents] = useState<EventsResponse>({ events: [] });
  const [runs, setRuns] = useState<RunsResponse>({ runs: [] });
  const [error, setError] = useState<string | null>(null);

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
              </li>
            ))}
          </ul>
        )}
      </section>

      {error ? <p className="error-banner">{error}</p> : null}
    </main>
  );
}
