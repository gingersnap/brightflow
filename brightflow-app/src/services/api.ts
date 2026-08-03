/**
 * REST API client
 */
import type {
  Action,
  ActionLogEntry,
  ActionManifestEntry,
  ActionResponse,
  AgentRunResponse,
  EnrichmentSettingsResponse,
  UpdateEnrichmentSettingsRequest,
  LlmProviderResponse,
  LlmTestResponse,
  UpsertLlmProviderRequest,
  AvailableConnectorResponse,
  BreakdownRow,
  BulkApproveResponse,
  BulkUndoResponse,
  ClusterDetail,
  ConnectorConfigResponse,
  CurationQueue,
  DashboardStats,
  DatasetInfo,
  EnrichedSyncRun,
  EventListRow,
  FunnelResult,
  InsightRunResponse,
  InsightsResponse,
  LoadTableResponse,
  PendingCount,
  QueryResponse,
  ReclusterRequest,
  RetentionResult,
  RunTriggerResponse,
  ScheduleResponse,
  Source,
  SyncRun,
  TaxonomyOverview,
  TextExploreRequest,
  TextExploreResponse,
  TimeseriesPoint,
  TopicsOverview,
  UnifiedConnector,
  User,
  UserProfile,
  UserTimelineEvent,
} from '@/types/generated';

const API_BASE = import.meta.env.VITE_API_BASE ?? '';

export class ApiError extends Error {
  status: number;
  data: unknown;

  constructor(message: string, status: number, data: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.data = data;
  }
}

interface RequestOptions extends Omit<RequestInit, 'body'> {
  body?: unknown;
}

async function request<T>(endpoint: string, options: RequestOptions = {}): Promise<T | null> {
  const url = `${API_BASE}${endpoint}`;

  const { body, ...restOptions } = options;
  const headers: Record<string, string> = { 'Content-Type': 'application/json' };
  if (
    restOptions.headers != null &&
    typeof restOptions.headers === 'object' &&
    !Array.isArray(restOptions.headers) &&
    !(restOptions.headers instanceof Headers)
  ) {
    Object.assign(headers, restOptions.headers);
  }
  const config: RequestInit = {
    credentials: 'include',
    headers,
    ...restOptions,
  };

  if (body != null && typeof body === 'object') {
    config.body = JSON.stringify(body);
  }

  const response = await fetch(url, config);

  if (!response.ok) {
    // Handle 401 for non-auth endpoints: clear auth state
    if (response.status === 401 && !endpoint.startsWith('/api/auth/')) {
      const { useAuthStore } = await import('@/stores/auth');
      const authStore = useAuthStore();
      authStore.clearAuth();
    }
    // oxlint-disable-next-line @typescript-eslint/no-unsafe-assignment -- JSON boundary
    const data: { message?: string; error?: { message?: string } } = await response
      .json()
      .catch(() => ({}));
    const message = data.error?.message ?? data.message ?? `Request failed: ${response.status}`;
    throw new ApiError(message, response.status, data);
  }

  // Handle empty responses
  const text = await response.text();
  // oxlint-disable-next-line @typescript-eslint/no-unsafe-return -- JSON boundary: parse returns any
  return text ? JSON.parse(text) : null;
}

export const api = {
  delete: <T>(endpoint: string, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'DELETE', ...options }),
  get: <T>(endpoint: string, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'GET', ...options }),
  post: <T>(endpoint: string, body?: unknown, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'POST', body, ...options }),
  put: <T>(endpoint: string, body?: unknown, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'PUT', body, ...options }),
};

// Re-export generated TableInfo from Rust backend
import type { TableInfo } from '@/types/generated/TableInfo';
export type { TableInfo } from '@/types/generated/TableInfo';

// Table API - for lazy loading Parquet tables
export const tableApi = {
  // Get list of available tables (metadata only, nothing loaded)
  listAvailable: (): Promise<TableInfo[] | null> => api.get<TableInfo[]>('/api/tables'),
  // Load a specific table into memory
  load: (sourceId: string, name: string): Promise<LoadTableResponse | null> =>
    api.post<LoadTableResponse>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(name)}/load`,
    ),
};

// Connector types
export type RunStatus = 'pending' | 'running' | 'completed' | 'failed';

// Connector API
export const connectApi = {
  // Discovery
  listAvailable: (): Promise<AvailableConnectorResponse[] | null> =>
    api.get<AvailableConnectorResponse[]>('/api/connectors/available'),
  listUnified: (): Promise<UnifiedConnector[] | null> =>
    api.get<UnifiedConnector[]>('/api/connectors/unified'),
  listRuns: (): Promise<EnrichedSyncRun[] | null> =>
    api.get<EnrichedSyncRun[]>('/api/connectors/runs'),
  listConnectorRuns: (name: string): Promise<SyncRun[] | null> =>
    api.get<SyncRun[]>(`/api/connectors/${encodeURIComponent(name)}/runs`),

  // Preset / connector config management
  createPreset: (data: {
    name: string;
    connectorPath: string;
    configJson: unknown;
    token?: string;
  }): Promise<{ id: string } | null> => api.post<{ id: string }>('/api/connector-configs', data),
  getConfig: (id: string): Promise<ConnectorConfigResponse | null> =>
    api.get<ConnectorConfigResponse>(`/api/connector-configs/${id}`),
  updateConfig: (
    id: string,
    data: { name?: string; connectorPath?: string; configJson?: unknown; token?: string },
  ): Promise<ConnectorConfigResponse | null> =>
    api.put<ConnectorConfigResponse>(`/api/connector-configs/${id}`, data),
  deleteConfig: (id: string): Promise<unknown> => api.delete(`/api/connector-configs/${id}`),
  runPreset: (id: string): Promise<RunTriggerResponse | null> =>
    api.post<RunTriggerResponse>(`/api/presets/${id}/run`),
  schedulePreset: (id: string, intervalSecs: number): Promise<ScheduleResponse | null> =>
    api.post<ScheduleResponse>(`/api/presets/${id}/schedule`, { intervalSecs }),

  // Legacy connector-name-based endpoints (still used)
  runConnector: (name: string): Promise<RunTriggerResponse | null> =>
    api.post<RunTriggerResponse>(`/api/connectors/${encodeURIComponent(name)}/run`),
  scheduleConnector: (name: string, intervalSecs: number): Promise<ScheduleResponse | null> =>
    api.post<ScheduleResponse>(`/api/connectors/${encodeURIComponent(name)}/schedule`, {
      intervalSecs,
    }),
  updateToken: (name: string, token: string): Promise<unknown> =>
    api.put(`/api/connectors/${encodeURIComponent(name)}/token`, { token }),
  deleteJob: (id: string): Promise<unknown> => api.delete(`/api/scheduler/jobs/${id}`),
  deleteSchedule: (id: string): Promise<unknown> => api.delete(`/api/schedules/${id}`),
  updateJob: (id: string, data: { intervalSecs?: number; enabled?: boolean }): Promise<unknown> =>
    api.put(`/api/scheduler/jobs/${id}`, data),
};

// Re-export generated insights types
export type { AnalysisTree } from '@/types/generated/AnalysisTree';
export type { AnalysisNode } from '@/types/generated/AnalysisNode';
export type { AnalysisType } from '@/types/generated/AnalysisType';
export type { NodeData } from '@/types/generated/NodeData';
export type { NodeId } from '@/types/generated/NodeId';
export type { ScoreBreakdown } from '@/types/generated/ScoreBreakdown';
export type { FilterStep } from '@/types/generated/FilterStep';
export type { TrendDirection } from '@/types/generated/TrendDirection';
export type { EngineConfig } from '@/types/generated/EngineConfig';

import type { EngineConfig } from '@/types/generated/EngineConfig';

// Insights API
export const insightsApi = {
  runReview: (params: {
    sourceId: string;
    datasetId: string;
    cadence?: string;
    config?: EngineConfig;
  }): Promise<InsightsResponse | null> =>
    api.post<InsightsResponse>('/api/insights/review', {
      cadence: params.cadence ?? 'weekly',
      config: params.config ?? {},
      datasetId: params.datasetId,
      sourceId: params.sourceId,
    }),
  runTrends: (params: {
    sourceId: string;
    datasetId: string;
    config?: EngineConfig;
  }): Promise<InsightsResponse | null> =>
    api.post<InsightsResponse>('/api/insights/trends', {
      config: params.config ?? {},
      datasetId: params.datasetId,
      sourceId: params.sourceId,
    }),
  runDrivers: (params: {
    sourceId: string;
    datasetId: string;
    config?: EngineConfig;
  }): Promise<InsightsResponse | null> =>
    api.post<InsightsResponse>('/api/insights/drivers', {
      config: params.config ?? {},
      datasetId: params.datasetId,
      sourceId: params.sourceId,
    }),
};

// Auth API
export const authApi = {
  login: (email: string, password: string): Promise<User | null> =>
    api.post<User>('/api/auth/login', { email, password }),
  logout: (): Promise<unknown> => api.post('/api/auth/logout'),
  me: (): Promise<User | null> => api.get<User>('/api/auth/me'),
};

// Dataset-specific API methods (for loaded datasets)
export const datasetApi = {
  delete: (id: string): Promise<unknown> => api.delete(`/api/datasets/${id}`),
  get: (id: string): Promise<DatasetInfo | null> => api.get<DatasetInfo>(`/api/datasets/${id}`),
  list: (): Promise<DatasetInfo[] | null> => api.get<DatasetInfo[]>('/api/datasets'),
  query: (datasetId: string, operations: unknown[]): Promise<QueryResponse | null> =>
    api.post<QueryResponse>('/api/query', { datasetId, operations }),
};

// Analytics Source API
import type { InsightHistoryRow, UnifiedSource } from '@/types';

export const sourceApi = {
  list: (): Promise<Source[] | null> => api.get<Source[]>('/api/sources'),
  unifiedList: (): Promise<UnifiedSource[] | null> =>
    api.get<UnifiedSource[]>('/api/sources/unified'),
  create: (domain: string, name: string): Promise<Source | null> =>
    api.post<Source>('/api/sources', { domain, name }),
  get: (id: string): Promise<Source | null> => api.get<Source>(`/api/sources/${id}`),
  update: (id: string, data: { name?: string; timezone?: string }): Promise<Source | null> =>
    api.put<Source>(`/api/sources/${id}`, data),
  delete: (id: string): Promise<unknown> => api.delete(`/api/sources/${id}`),
  snippet: (id: string): Promise<{ snippet: string } | null> =>
    api.get<{ snippet: string }>(`/api/sources/${id}/snippet`),
  /** Persist a CSV as a first-class upload source (survives restarts). */
  uploadCsv: async (file: File, table?: string): Promise<UploadSourceResult> => {
    const formData = new FormData();
    formData.append('file', file);
    if (table != null && table.trim() !== '') {
      formData.append('table', table.trim());
    }
    const response = await fetch(`${API_BASE}/api/sources/upload`, {
      method: 'POST',
      body: formData,
      credentials: 'include',
    });
    if (!response.ok) {
      // oxlint-disable-next-line @typescript-eslint/no-unsafe-assignment -- JSON boundary
      const data: { message?: string; error?: { message?: string } } = await response
        .json()
        .catch(() => ({}));
      throw new ApiError(
        data.error?.message ?? data.message ?? 'Upload failed',
        response.status,
        data,
      );
    }
    // oxlint-disable-next-line @typescript-eslint/no-unsafe-return -- JSON boundary
    return response.json();
  },
};

// Analytics Dashboard API
export const analyticsApi = {
  stats: (sourceId: string, period = '30d'): Promise<DashboardStats | null> =>
    api.get<DashboardStats>(`/api/analytics/${sourceId}/stats?period=${period}`),
  timeseries: (sourceId: string, period = '30d'): Promise<TimeseriesPoint[] | null> =>
    api.get<TimeseriesPoint[]>(`/api/analytics/${sourceId}/timeseries?period=${period}`),
  topPages: (sourceId: string, period = '30d'): Promise<BreakdownRow[] | null> =>
    api.get<BreakdownRow[]>(`/api/analytics/${sourceId}/top-pages?period=${period}`),
  referrers: (sourceId: string, period = '30d'): Promise<BreakdownRow[] | null> =>
    api.get<BreakdownRow[]>(`/api/analytics/${sourceId}/referrers?period=${period}`),
  utm: (sourceId: string, period = '30d'): Promise<BreakdownRow[] | null> =>
    api.get<BreakdownRow[]>(`/api/analytics/${sourceId}/utm?period=${period}`),
  devices: (sourceId: string, period = '30d'): Promise<BreakdownRow[] | null> =>
    api.get<BreakdownRow[]>(`/api/analytics/${sourceId}/devices?period=${period}`),
  geo: (sourceId: string, period = '30d'): Promise<BreakdownRow[] | null> =>
    api.get<BreakdownRow[]>(`/api/analytics/${sourceId}/geo?period=${period}`),
};

// Product Analytics API
export const productAnalyticsApi = {
  events: (sourceId: string, period = '30d'): Promise<EventListRow[] | null> =>
    api.get<EventListRow[]>(`/api/analytics/${sourceId}/events?period=${period}`),
  funnel: (
    sourceId: string,
    body: { steps: { name: string }[]; windowSeconds: number; period: string },
  ): Promise<FunnelResult | null> =>
    api.post<FunnelResult>(`/api/analytics/${sourceId}/funnel`, body),
  retention: (
    sourceId: string,
    body: {
      cohortEvent: string;
      returnEvent: string;
      periodType: string;
      numPeriods: number;
      period: string;
    },
  ): Promise<RetentionResult | null> =>
    api.post<RetentionResult>(`/api/analytics/${sourceId}/retention`, body),
  searchUsers: (sourceId: string, q = '', limit = 20): Promise<UserProfile[] | null> =>
    api.get<UserProfile[]>(
      `/api/analytics/${sourceId}/users?q=${encodeURIComponent(q)}&limit=${limit}`,
    ),
  userTimeline: (sourceId: string, userId: string): Promise<UserTimelineEvent[] | null> =>
    api.get<UserTimelineEvent[]>(
      `/api/analytics/${sourceId}/users/${encodeURIComponent(userId)}/timeline`,
    ),
  userProfile: (sourceId: string, userId: string): Promise<UserProfile | null> =>
    api.get<UserProfile>(`/api/analytics/${sourceId}/users/${encodeURIComponent(userId)}/profile`),
};

// Topics API (Model2Vec embeddings + dense k-means)
export const topicsApi = {
  overview: (sourceId: string, table: string): Promise<TopicsOverview | null> =>
    api.get<TopicsOverview>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/topics`,
    ),
  clusterDetail: (
    sourceId: string,
    table: string,
    clusterId: number,
  ): Promise<ClusterDetail | null> =>
    api.get<ClusterDetail>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/topics/clusters/${clusterId}`,
    ),
  recluster: (
    sourceId: string,
    table: string,
    body: ReclusterRequest = {},
  ): Promise<TopicsOverview | null> =>
    api.post<TopicsOverview>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/topics/recluster`,
      body,
    ),
  getEnrichment: (sourceId: string, table: string): Promise<EnrichmentSettingsResponse | null> =>
    api.get<EnrichmentSettingsResponse>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/enrichment`,
    ),
  updateEnrichment: (
    sourceId: string,
    table: string,
    body: UpdateEnrichmentSettingsRequest,
  ): Promise<EnrichmentSettingsResponse | null> =>
    api.put<EnrichmentSettingsResponse>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/enrichment`,
      body,
    ),
};

// Text Explorer: one POST returns filtered rows + words widget together.
export const textExploreApi = {
  search: (
    sourceId: string,
    table: string,
    body: TextExploreRequest,
  ): Promise<TextExploreResponse | null> =>
    api.post<TextExploreResponse>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/textexplore/search`,
      body,
    ),
};

// Intent taxonomy: read-only. Every write goes through actionsApi.dispatch so
// Human edits and agent proposals share one path, one audit log and one undo.
export const taxonomyApi = {
  overview: (sourceId: string, table: string): Promise<TaxonomyOverview | null> =>
    api.get<TaxonomyOverview>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/taxonomy`,
    ),
  queue: (
    sourceId: string,
    table: string,
    params: { limit?: number; filter?: string } = {},
  ): Promise<CurationQueue | null> => {
    const search = new URLSearchParams();
    if (params.limit != null) {
      search.set('limit', String(params.limit));
    }
    if (params.filter != null) {
      search.set('filter', params.filter);
    }
    const query = search.size > 0 ? `?${search.toString()}` : '';
    return api.get<CurationQueue>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/taxonomy/queue${query}`,
    );
  },
};

// Insight history (novelty memory) inspection + reset.
// The rows come from the store layer (serde snake_case), not ts-rs — the
// Interface lives in `@/types` (imported above with UnifiedSource).
export const insightHistoryApi = {
  list: (sourceId: string, table: string): Promise<InsightHistoryRow[] | null> =>
    api.get<InsightHistoryRow[]>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/insights/history`,
    ),
  reset: (sourceId: string, table: string): Promise<{ deleted: number } | null> =>
    api.delete<{ deleted: number }>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/insights/history`,
    ),
};

// Insight runs (auto/manual computation log — badge + history panel)
export const insightRunsApi = {
  list: (sourceId: string, table: string, limit = 25): Promise<InsightRunResponse[] | null> =>
    api.get<InsightRunResponse[]>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/insights/runs?limit=${limit}`,
    ),
  /** Latest run per table of a source — badge hydration on load/reconnect. */
  latest: (sourceId: string): Promise<InsightRunResponse[] | null> =>
    api.get<InsightRunResponse[]>(`/api/sources/${encodeURIComponent(sourceId)}/insights/latest`),
};

// First-class curation actions: one dispatch path for humans and agents
export const actionsApi = {
  dispatch: (action: Action, requestId: string): Promise<ActionResponse | null> =>
    api.post<ActionResponse>('/api/actions', { action, requestId }),
  /** Action catalog: kind, label, description, undoability, param schema. */
  manifest: (): Promise<ActionManifestEntry[] | null> =>
    api.get<ActionManifestEntry[]>('/api/actions/manifest'),
  feed: (limit = 100): Promise<ActionLogEntry[] | null> =>
    api.get<ActionLogEntry[]>(`/api/actions?limit=${limit}`),
  approve: (id: number): Promise<ActionResponse | null> =>
    api.post<ActionResponse>(`/api/actions/${id}/approve`),
  /** Approve every pending proposal, oldest first. */
  approveAll: (): Promise<BulkApproveResponse | null> =>
    api.post<BulkApproveResponse>('/api/actions/approve-all'),
  /** True pending count — the feed is truncated, so don't count it client-side. */
  pendingCount: (): Promise<PendingCount | null> =>
    api.get<PendingCount>('/api/actions/pending-count'),
  reject: (id: number): Promise<ActionResponse | null> =>
    api.post<ActionResponse>(`/api/actions/${id}/reject`),
  undo: (id: number): Promise<ActionResponse | null> =>
    api.post<ActionResponse>(`/api/actions/${id}/undo`),
};

// LLM provider settings (optional, OpenAI-compatible endpoints)
export const llmApi = {
  listProviders: (): Promise<LlmProviderResponse[] | null> =>
    api.get<LlmProviderResponse[]>('/api/llm/providers'),
  upsertProvider: (body: UpsertLlmProviderRequest): Promise<LlmProviderResponse[] | null> =>
    api.post<LlmProviderResponse[]>('/api/llm/providers', body),
  deleteProvider: (id: number): Promise<LlmProviderResponse[] | null> =>
    api.delete<LlmProviderResponse[]>(`/api/llm/providers/${id}`),
  testProvider: (id: number): Promise<LlmTestResponse | null> =>
    api.post<LlmTestResponse>(`/api/llm/providers/${id}/test`),
};

// Enrichment functions: versioned derived columns (llm_prompt / topic_model)
import type {
  EnrichEstimate,
  EnrichFunction,
  EnrichRun,
  FunctionVersion,
  RunScope,
  SampleRunResult,
  UploadSourceResult,
} from '@/types/enrichment';

export const enrichFnApi = {
  list: (sourceId: string, table: string): Promise<EnrichFunction[] | null> =>
    api.get<EnrichFunction[]>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/functions`,
    ),
  create: (
    sourceId: string,
    table: string,
    body: { name: string; kind: string; config: unknown },
  ): Promise<EnrichFunction | null> =>
    api.post<EnrichFunction>(
      `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}/functions`,
      body,
    ),
  get: (id: string): Promise<EnrichFunction | null> =>
    api.get<EnrichFunction>(`/api/functions/${encodeURIComponent(id)}`),
  update: (
    id: string,
    body: { config: unknown; rerun?: 'none' | 'all' | 'missing' },
  ): Promise<EnrichFunction | null> =>
    api.put<EnrichFunction>(`/api/functions/${encodeURIComponent(id)}`, body),
  delete: (id: string, dropColumns: boolean): Promise<unknown> =>
    api.delete(`/api/functions/${encodeURIComponent(id)}?drop_columns=${String(dropColumns)}`),
  versions: (id: string): Promise<FunctionVersion[] | null> =>
    api.get<FunctionVersion[]>(`/api/functions/${encodeURIComponent(id)}/versions`),
  promote: (id: string): Promise<EnrichFunction | null> =>
    api.post<EnrichFunction>(`/api/functions/${encodeURIComponent(id)}/promote`),
  demote: (id: string): Promise<EnrichFunction | null> =>
    api.post<EnrichFunction>(`/api/functions/${encodeURIComponent(id)}/demote`),
  sampleRun: (
    id: string,
    body: { limit?: number; config?: unknown },
  ): Promise<SampleRunResult | null> =>
    api.post<SampleRunResult>(`/api/functions/${encodeURIComponent(id)}/sample-run`, body),
  estimate: (id: string, scope: RunScope): Promise<EnrichEstimate | null> =>
    api.get<EnrichEstimate>(`/api/functions/${encodeURIComponent(id)}/estimate?scope=${scope}`),
  startRun: (id: string, scope: RunScope): Promise<{ runId: string } | null> =>
    api.post<{ runId: string }>(`/api/functions/${encodeURIComponent(id)}/runs`, { scope }),
  getRun: (runId: string): Promise<EnrichRun | null> =>
    api.get<EnrichRun>(`/api/enrichment/runs/${encodeURIComponent(runId)}`),
  cancelRun: (runId: string): Promise<EnrichRun | null> =>
    api.post<EnrichRun>(`/api/enrichment/runs/${encodeURIComponent(runId)}/cancel`),
};

// Agent runs: LLM curation through the same action layer
export const agentApi = {
  start: (req: {
    kind: string;
    sourceId: string;
    table: string;
    /** "auto_apply" (default) or "propose". */
    mode?: 'auto_apply' | 'propose';
  }): Promise<AgentRunResponse | null> => api.post<AgentRunResponse>('/api/agent/runs', req),
  /** Revert every applied, undoable action of a run, newest first. */
  undoAll: (id: number): Promise<BulkUndoResponse | null> =>
    api.post<BulkUndoResponse>(`/api/agent/runs/${id}/undo-all`),
  list: (limit = 50): Promise<AgentRunResponse[] | null> =>
    api.get<AgentRunResponse[]>(`/api/agent/runs?limit=${limit}`),
  get: (id: number): Promise<AgentRunResponse | null> =>
    api.get<AgentRunResponse>(`/api/agent/runs/${id}`),
  cancel: (id: number): Promise<AgentRunResponse | null> =>
    api.post<AgentRunResponse>(`/api/agent/runs/${id}/cancel`),
};
