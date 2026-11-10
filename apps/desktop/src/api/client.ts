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

export interface LifecycleTextRequest {
  text: string;
}

export interface RunLifecycleResponse {
  run_id: string;
}

export type ToolEffect = 'read_only' | 'local_write' | 'network' | 'shell';

export type ToolApprovalDecision = 'auto_approved' | 'requires_approval';

export interface ToolCallRequest {
  tool_name: string;
  effect: ToolEffect;
  input_json: unknown;
}

export interface ToolCallResponse {
  run_id: string;
  tool_call_id: string;
  approval: ToolApprovalDecision;
}

export interface ToolLifecycleRequest {
  text: string;
}

export interface ToolResultRequest {
  output_json: unknown;
}

export interface ToolLifecycleResponse {
  run_id: string;
  tool_call_id: string;
}

export type ArtifactKind = 'text' | 'markdown' | 'json' | 'binary';

export interface ArtifactRecord {
  artifact_id: string;
  run_id: string;
  title: string;
  kind: ArtifactKind;
  mime_type: string;
  sha256: string;
  byte_len: number;
  created_at: string;
}

export interface ArtifactDetail {
  artifact: ArtifactRecord;
  content: string | null;
}

export interface CreateArtifactRequest {
  title: string;
  kind: ArtifactKind;
  mime_type: string;
  content: string;
}

export interface CreateArtifactResponse {
  artifact_id: string;
}

export interface ArtifactsResponse {
  artifacts: ArtifactRecord[];
}

export interface ContextBundleRecord {
  context_bundle_id: string;
  run_id: string;
  title: string;
  summary: string;
  artifact_ids: string[];
  event_ids: string[];
  created_at: string;
}

export interface CreateContextBundleRequest {
  title: string;
  summary: string;
  artifact_ids: string[];
  event_ids: string[];
}

export interface CreateContextBundleResponse {
  context_bundle_id: string;
}

export interface ContextBundlesResponse {
  context_bundles: ContextBundleRecord[];
}

export interface MnemeApiClient {
  getHealth(): Promise<KernelHealth>;
  listEvents(): Promise<EventsResponse>;
  listRuns(): Promise<RunsResponse>;
  createRun(instruction: string): Promise<CreateRunResponse>;
  requestModel(runId: string, text: string): Promise<RunLifecycleResponse>;
  recordAssistantMessage(runId: string, text: string): Promise<RunLifecycleResponse>;
  completeRun(runId: string, text: string): Promise<RunLifecycleResponse>;
  failRun(runId: string, text: string): Promise<RunLifecycleResponse>;
  requestToolCall(runId: string, request: ToolCallRequest): Promise<ToolCallResponse>;
  approveToolCall(runId: string, toolCallId: string, text: string): Promise<ToolLifecycleResponse>;
  denyToolCall(runId: string, toolCallId: string, text: string): Promise<ToolLifecycleResponse>;
  recordToolResult(
    runId: string,
    toolCallId: string,
    outputJson: unknown,
  ): Promise<ToolLifecycleResponse>;
  recordToolFailure(runId: string, toolCallId: string, text: string): Promise<ToolLifecycleResponse>;
  listArtifacts(): Promise<ArtifactsResponse>;
  createArtifact(runId: string, request: CreateArtifactRequest): Promise<CreateArtifactResponse>;
  getArtifact(artifactId: string): Promise<ArtifactDetail>;
  listContextBundles(): Promise<ContextBundlesResponse>;
  createContextBundle(
    runId: string,
    request: CreateContextBundleRequest,
  ): Promise<CreateContextBundleResponse>;
  getContextBundle(contextBundleId: string): Promise<ContextBundleRecord>;
}

type FetchLike = typeof fetch;

async function parseJsonResponse<T>(response: Response): Promise<T> {
  if (!response.ok) {
    throw new Error(`Mneme API request failed with status ${response.status}`);
  }
  return (await response.json()) as T;
}

async function postLifecycleText(
  normalizedBaseUrl: string,
  fetchImpl: FetchLike,
  runId: string,
  suffix: string,
  text: string,
): Promise<RunLifecycleResponse> {
  const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/runs/${runId}/${suffix}`, {
    method: 'POST',
    headers: { accept: 'application/json', 'content-type': 'application/json' },
    body: JSON.stringify({ text }),
  });
  return parseJsonResponse<RunLifecycleResponse>(response);
}

async function postToolText(
  normalizedBaseUrl: string,
  fetchImpl: FetchLike,
  runId: string,
  toolCallId: string,
  suffix: string,
  text: string,
): Promise<ToolLifecycleResponse> {
  const response = await fetchImpl(
    `${normalizedBaseUrl}/api/v1/runs/${runId}/tool-calls/${toolCallId}/${suffix}`,
    {
      method: 'POST',
      headers: { accept: 'application/json', 'content-type': 'application/json' },
      body: JSON.stringify({ text }),
    },
  );
  return parseJsonResponse<ToolLifecycleResponse>(response);
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

    requestModel(runId: string, text: string) {
      return postLifecycleText(normalizedBaseUrl, fetchImpl, runId, 'model-requests', text);
    },

    recordAssistantMessage(runId: string, text: string) {
      return postLifecycleText(normalizedBaseUrl, fetchImpl, runId, 'assistant-messages', text);
    },

    completeRun(runId: string, text: string) {
      return postLifecycleText(normalizedBaseUrl, fetchImpl, runId, 'complete', text);
    },

    failRun(runId: string, text: string) {
      return postLifecycleText(normalizedBaseUrl, fetchImpl, runId, 'fail', text);
    },

    async requestToolCall(runId: string, request: ToolCallRequest) {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/runs/${runId}/tool-calls`, {
        method: 'POST',
        headers: { accept: 'application/json', 'content-type': 'application/json' },
        body: JSON.stringify(request),
      });
      return parseJsonResponse<ToolCallResponse>(response);
    },

    approveToolCall(runId: string, toolCallId: string, text: string) {
      return postToolText(normalizedBaseUrl, fetchImpl, runId, toolCallId, 'approve', text);
    },

    denyToolCall(runId: string, toolCallId: string, text: string) {
      return postToolText(normalizedBaseUrl, fetchImpl, runId, toolCallId, 'deny', text);
    },

    async recordToolResult(runId: string, toolCallId: string, outputJson: unknown) {
      const response = await fetchImpl(
        `${normalizedBaseUrl}/api/v1/runs/${runId}/tool-calls/${toolCallId}/result`,
        {
          method: 'POST',
          headers: { accept: 'application/json', 'content-type': 'application/json' },
          body: JSON.stringify({ output_json: outputJson }),
        },
      );
      return parseJsonResponse<ToolLifecycleResponse>(response);
    },

    recordToolFailure(runId: string, toolCallId: string, text: string) {
      return postToolText(normalizedBaseUrl, fetchImpl, runId, toolCallId, 'fail', text);
    },

    async listArtifacts() {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/artifacts`, {
        headers: { accept: 'application/json' },
      });
      return parseJsonResponse<ArtifactsResponse>(response);
    },

    async createArtifact(runId: string, request: CreateArtifactRequest) {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/runs/${runId}/artifacts`, {
        method: 'POST',
        headers: { accept: 'application/json', 'content-type': 'application/json' },
        body: JSON.stringify(request),
      });
      return parseJsonResponse<CreateArtifactResponse>(response);
    },

    async getArtifact(artifactId: string) {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/artifacts/${artifactId}`, {
        headers: { accept: 'application/json' },
      });
      return parseJsonResponse<ArtifactDetail>(response);
    },

    async listContextBundles() {
      const response = await fetchImpl(`${normalizedBaseUrl}/api/v1/context-bundles`, {
        headers: { accept: 'application/json' },
      });
      return parseJsonResponse<ContextBundlesResponse>(response);
    },

    async createContextBundle(runId: string, request: CreateContextBundleRequest) {
      const response = await fetchImpl(
        `${normalizedBaseUrl}/api/v1/runs/${runId}/context-bundles`,
        {
          method: 'POST',
          headers: { accept: 'application/json', 'content-type': 'application/json' },
          body: JSON.stringify(request),
        },
      );
      return parseJsonResponse<CreateContextBundleResponse>(response);
    },

    async getContextBundle(contextBundleId: string) {
      const response = await fetchImpl(
        `${normalizedBaseUrl}/api/v1/context-bundles/${contextBundleId}`,
        {
          headers: { accept: 'application/json' },
        },
      );
      return parseJsonResponse<ContextBundleRecord>(response);
    },
  };
}
