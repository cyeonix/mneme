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
