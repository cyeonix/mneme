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
    async listRuns() {
      return {
        runs: [
          {
            run_id: 'run-1',
            status: 'running',
            instruction: '展示 runs 投影',
            started_at: '2026-11-09T00:00:00Z',
            completed_at: null,
            failed_at: null,
            summary: null,
            error: null,
            updated_at: '2026-11-09T00:00:00Z',
          },
        ],
      };
    },
    async createRun() {
      return { run_id: 'run-123' };
    },
    async requestModel() {
      return { run_id: 'run-123' };
    },
    async recordAssistantMessage() {
      return { run_id: 'run-123' };
    },
    async completeRun() {
      return { run_id: 'run-123' };
    },
    async failRun() {
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
    expect(await screen.findByText('展示 runs 投影')).toBeInTheDocument();
    expect(screen.getByText('running')).toBeInTheDocument();
  });
});
