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

export type RunStatus = 'running' | 'completed' | 'failed';

export interface RunProjection {
  run_id: string;
  status: RunStatus;
  instruction: string;
  started_at: string;
  completed_at: string | null;
  failed_at: string | null;
  summary: string | null;
  error: string | null;
  updated_at: string;
}

export interface RunsResponse {
  runs: RunProjection[];
}

export interface CreateRunResponse {
  run_id: string;
}

export interface MnemeApiClient {
  getHealth(): Promise<KernelHealth>;
  listEvents(): Promise<EventsResponse>;
  listRuns(): Promise<RunsResponse>;
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

    async listRuns() {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/runs`, {
        headers: { accept: 'application/json' },
      });
      return parseJsonResponse<RunsResponse>(response);
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
