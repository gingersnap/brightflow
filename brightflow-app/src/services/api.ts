/**
 * REST API client
 */
import type {
  DatasetInfo,
  InsightsResponse,
  LoadTableResponse,
  QueryResponse,
  RunTriggerResponse,
  ScheduleResponse,
  SyncRun,
  UnifiedConnector,
  UploadResponse,
  User,
} from '@/types/generated';

const API_BASE = import.meta.env.VITE_API_BASE || '';

export class ApiError extends Error {
  status: number;
  data: unknown;

  constructor(message: string, status: number, data: unknown) {
    super(message);
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
  const config: RequestInit = {
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      ...(restOptions.headers as Record<string, string>),
    },
    ...restOptions,
  };

  if (body && typeof body === 'object') {
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
    const data = (await response.json().catch(() => ({}))) as { message?: string };
    throw new ApiError(data.message ?? `Request failed: ${response.status}`, response.status, data);
  }

  // Handle empty responses
  const text = await response.text();
  return text ? (JSON.parse(text) as T) : null;
}

export const api = {
  delete: async <T>(endpoint: string, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'DELETE', ...options }),
  get: async <T>(endpoint: string, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'GET', ...options }),
  post: async <T>(endpoint: string, body?: unknown, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'POST', body, ...options }),
  put: async <T>(endpoint: string, body?: unknown, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'PUT', body, ...options }),
};

// Re-export generated TableInfo from Rust backend
import type { TableInfo } from '@/types/generated/TableInfo';
export type { TableInfo };

// Table API - for lazy loading Parquet tables
export const tableApi = {
  // Get list of available tables (metadata only, nothing loaded)
  listAvailable: async (): Promise<TableInfo[] | null> => api.get<TableInfo[]>('/api/tables'),
  // Load a specific table into memory
  load: async (name: string): Promise<LoadTableResponse | null> =>
    api.post<LoadTableResponse>(`/api/tables/${encodeURIComponent(name)}/load`),
};

// Connector types
export type RunStatus = 'pending' | 'running' | 'completed' | 'failed';

// Connector API
export const connectApi = {
  deleteJob: async (id: string): Promise<unknown> => api.delete(`/api/scheduler/jobs/${id}`),
  listConnectorRuns: async (name: string): Promise<SyncRun[] | null> =>
    api.get<SyncRun[]>(`/api/connectors/${encodeURIComponent(name)}/runs`),
  listUnified: async (): Promise<UnifiedConnector[] | null> =>
    api.get<UnifiedConnector[]>('/api/connectors/unified'),
  runConnector: async (name: string): Promise<RunTriggerResponse | null> =>
    api.post<RunTriggerResponse>(`/api/connectors/${encodeURIComponent(name)}/run`),
  scheduleConnector: async (name: string, intervalSecs: number): Promise<ScheduleResponse | null> =>
    api.post<ScheduleResponse>(`/api/connectors/${encodeURIComponent(name)}/schedule`, {
      intervalSecs,
    }),
  updateJob: async (
    id: string,
    data: { intervalSecs?: number; enabled?: boolean },
  ): Promise<unknown> => api.put(`/api/scheduler/jobs/${id}`, data),
  updateToken: async (name: string, token: string): Promise<unknown> =>
    api.put(`/api/connectors/${encodeURIComponent(name)}/token`, { token }),
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
  runReview: async (datasetId: string, cadence = 'weekly'): Promise<InsightsResponse | null> =>
    api.post<InsightsResponse>('/api/insights/review', { cadence, datasetId }),
  runTrends: async (datasetId: string): Promise<InsightsResponse | null> =>
    api.post<InsightsResponse>('/api/insights/trends', { datasetId }),
};

// Auth API
export const authApi = {
  login: async (email: string, password: string): Promise<User | null> =>
    api.post<User>('/api/auth/login', { email, password }),
  logout: async (): Promise<unknown> => api.post('/api/auth/logout'),
  me: async (): Promise<User | null> => api.get<User>('/api/auth/me'),
};

// Dataset-specific API methods (for loaded datasets)
export const datasetApi = {
  delete: async (id: string): Promise<unknown> => api.delete(`/api/datasets/${id}`),
  get: async (id: string): Promise<DatasetInfo | null> =>
    api.get<DatasetInfo>(`/api/datasets/${id}`),
  list: async (): Promise<DatasetInfo[] | null> => api.get<DatasetInfo[]>('/api/datasets'),
  query: async (datasetId: string, operations: unknown[]): Promise<QueryResponse | null> =>
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
      const data = (await response.json().catch(() => ({}))) as { message?: string };
      throw new ApiError(data.message ?? 'Upload failed', response.status, data);
    }

    return response.json() as Promise<UploadResponse>;
  },
};
