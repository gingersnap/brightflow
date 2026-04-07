/**
 * REST API client
 */
import type {
  AvailableConnectorResponse,
  BreakdownRow,
  EnrichedSyncRun,
  DashboardStats,
  DatasetInfo,
  EventListRow,
  FunnelResult,
  InsightsResponse,
  LoadTableResponse,
  QueryResponse,
  RetentionResult,
  RunTriggerResponse,
  ScheduleResponse,
  Source,
  SyncRun,
  TimeseriesPoint,
  UnifiedConnector,
  UploadResponse,
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
    const data: { message?: string } = await response.json().catch(() => ({}));
    throw new ApiError(data.message ?? `Request failed: ${response.status}`, response.status, data);
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
export type { TableInfo };

// Table API - for lazy loading Parquet tables
export const tableApi = {
  // Get list of available tables (metadata only, nothing loaded)
  listAvailable: (): Promise<TableInfo[] | null> => api.get<TableInfo[]>('/api/tables'),
  // Load a specific table into memory
  load: (name: string): Promise<LoadTableResponse | null> =>
    api.post<LoadTableResponse>(`/api/tables/${encodeURIComponent(name)}/load`),
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

  // Preset management
  createPreset: (data: {
    name: string;
    connectorPath: string;
    configJson: unknown;
    token?: string;
  }): Promise<unknown> => api.post('/api/connector-configs', data),
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

// Frontend-specific tree types (refinements of the backend's serde_json::Value)
export interface AnalysisTree {
  nodes: AnalysisNode[];
  roots: { '0': number }[];
}

export interface AnalysisNode {
  id: { '0': number };
  parent_id: { '0': number } | null;
  analysis: AnalysisType;
  significance: number;
  description: string;
  summary: string;
  tech_summary: string;
  children: { '0': number }[];
}

export interface AnalysisType {
  type: string;
  [key: string]: unknown;
}

// Insights API
export const insightsApi = {
  runReview: (datasetId: string, cadence = 'weekly'): Promise<InsightsResponse | null> =>
    api.post<InsightsResponse>('/api/insights/review', { cadence, datasetId }),
  runTrends: (datasetId: string): Promise<InsightsResponse | null> =>
    api.post<InsightsResponse>('/api/insights/trends', { datasetId }),
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
  upload: async (file: File): Promise<UploadResponse> => {
    const formData = new FormData();
    formData.append('file', file);

    const response = await fetch(`${API_BASE}/api/datasets/upload`, {
      method: 'POST',
      body: formData,
      credentials: 'include',
    });

    if (!response.ok) {
      // oxlint-disable-next-line @typescript-eslint/no-unsafe-assignment -- JSON boundary
      const data: { message?: string } = await response.json().catch(() => ({}));
      throw new ApiError(data.message ?? 'Upload failed', response.status, data);
    }

    // oxlint-disable-next-line @typescript-eslint/no-unsafe-return -- JSON boundary
    return response.json();
  },
};

// Analytics Source API
export const sourceApi = {
  list: (): Promise<Source[] | null> => api.get<Source[]>('/api/sources'),
  create: (domain: string, name: string): Promise<Source | null> =>
    api.post<Source>('/api/sources', { domain, name }),
  get: (id: string): Promise<Source | null> => api.get<Source>(`/api/sources/${id}`),
  update: (id: string, data: { name?: string; timezone?: string }): Promise<Source | null> =>
    api.put<Source>(`/api/sources/${id}`, data),
  delete: (id: string): Promise<unknown> => api.delete(`/api/sources/${id}`),
  snippet: (id: string): Promise<{ snippet: string } | null> =>
    api.get<{ snippet: string }>(`/api/sources/${id}/snippet`),
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
