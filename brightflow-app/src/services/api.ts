/**
 * REST API client
 */
const API_BASE = import.meta.env.VITE_API_BASE || 'http://localhost:8080';

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
    headers: {
      'Content-Type': 'application/json',
      ...restOptions.headers,
    },
    credentials: 'include',
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
  get: <T>(endpoint: string, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'GET', ...options }),
  post: <T>(endpoint: string, body?: unknown, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'POST', body, ...options }),
  put: <T>(endpoint: string, body?: unknown, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'PUT', body, ...options }),
  delete: <T>(endpoint: string, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'DELETE', ...options }),
};

interface DatasetInfo {
  id: string;
  name: string;
  rowCount: number;
  columnCount: number;
  loadedAt: string;
}

// Available table from Delta store (metadata only, not loaded)
export interface TableInfo {
  name: string;
  path: string;
  version: number;
  num_rows: number | null;
  num_files: number;
}

// Response when loading a table
export interface LoadTableResponse {
  id: string;
  name: string;
  rowCount: number;
  columnCount: number;
  columns: Array<{ name: string; dtype: string }>;
}

interface UploadResponse {
  id: string;
  name: string;
  row_count: number;
}

interface QueryResponse {
  columns: Array<{ name: string; dtype: string }>;
  rows: unknown[][];
  row_count: number;
  total_rows: number;
}

// Table API - for lazy loading Delta tables
export const tableApi = {
  // Get list of available tables (metadata only, nothing loaded)
  listAvailable: (): Promise<TableInfo[] | null> => api.get<TableInfo[]>('/api/tables'),
  // Load a specific table into memory
  load: (name: string): Promise<LoadTableResponse | null> =>
    api.post<LoadTableResponse>(`/api/tables/${encodeURIComponent(name)}/load`),
};

// Connector types
export interface ConnectorInfo {
  name: string;
  connector: string;
  valid: boolean;
}

export type RunStatus = 'pending' | 'running' | 'completed' | 'failed';

export interface ConnectorRun {
  id: string;
  connector: string;
  config_name: string;
  status: RunStatus;
  started_at: string;
  finished_at: string | null;
  endpoints_synced: string[];
  tables_ingested: string[];
  error: string | null;
}

export interface RunResponse {
  run_id: string;
  connector: string;
  status: RunStatus;
}

// Connector API
export const connectApi = {
  listConnectors: (): Promise<ConnectorInfo[] | null> =>
    api.get<ConnectorInfo[]>('/api/connectors'),
  runConnector: (name: string, only?: string): Promise<RunResponse | null> =>
    api.post<RunResponse>(`/api/connectors/${encodeURIComponent(name)}/run`, only ? { only } : {}),
  listRuns: (): Promise<ConnectorRun[] | null> => api.get<ConnectorRun[]>('/api/connectors/runs'),
  getRun: (id: string): Promise<ConnectorRun | null> =>
    api.get<ConnectorRun>(`/api/connectors/runs/${id}`),
};

// Insights API response
export interface InsightsResponse {
  datasetId: string;
  reportType: string;
  tree: AnalysisTree;
  nodeCount: number;
  findingCount: number;
  executionTimeMs: number;
}

export interface AnalysisTree {
  nodes: AnalysisNode[];
  roots: Array<{ '0': number }>;
}

export interface AnalysisNode {
  id: { '0': number };
  parent_id: { '0': number } | null;
  analysis: AnalysisType;
  significance: number;
  description: string;
  summary: string;
  tech_summary: string;
  children: Array<{ '0': number }>;
}

export type AnalysisType = {
  type: string;
  [key: string]: unknown;
};

// Insights API
export const insightsApi = {
  runReview: (datasetId: string, cadence: string = 'weekly'): Promise<InsightsResponse | null> =>
    api.post<InsightsResponse>('/api/insights/review', { datasetId, cadence }),
  runTrends: (datasetId: string): Promise<InsightsResponse | null> =>
    api.post<InsightsResponse>('/api/insights/trends', { datasetId }),
};

// Auth types
interface AuthUser {
  id: string;
  email: string;
  displayName: string;
  isAdmin: boolean;
}

// Auth API
export const authApi = {
  login: (email: string, password: string): Promise<AuthUser | null> =>
    api.post<AuthUser>('/api/auth/login', { email, password }),
  logout: (): Promise<unknown> => api.post('/api/auth/logout'),
  me: (): Promise<AuthUser | null> => api.get<AuthUser>('/api/auth/me'),
};

// Dataset-specific API methods (for loaded datasets)
export const datasetApi = {
  list: (): Promise<DatasetInfo[] | null> => api.get<DatasetInfo[]>('/api/datasets'),
  get: (id: string): Promise<DatasetInfo | null> => api.get<DatasetInfo>(`/api/datasets/${id}`),
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
  delete: (id: string): Promise<unknown> => api.delete(`/api/datasets/${id}`),
  query: (datasetId: string, operations: unknown[]): Promise<QueryResponse | null> =>
    api.post<QueryResponse>('/api/query', { datasetId, operations }),
};
