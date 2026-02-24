/**
 * Scheduler & Sync API client
 */
import { api } from './api';

// --- Types ---

export interface ConnectorConfig {
  id: string;
  name: string;
  connectorPath: string;
  configJson: string;
  createdAt: string;
  updatedAt: string;
}

export interface SchedulerJob {
  id: string;
  name: string;
  connectorId: string;
  intervalSecs: number;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface SyncRun {
  id: string;
  jobId: string | null;
  connectorId: string;
  startedAt: string;
  finishedAt: string | null;
  status: 'pending' | 'running' | 'completed' | 'failed';
  endpointsSynced: string | null;
  rowsSynced: number;
  error: string | null;
}

export interface SyncState {
  connectorId: string;
  endpoint: string;
  cursorField: string | null;
  cursorValue: string | null;
  lastSyncAt: string | null;
  lastSyncStatus: string | null;
  rowsSynced: number;
}

// --- Connector Config API ---

export const connectorConfigApi = {
  list: (): Promise<ConnectorConfig[] | null> =>
    api.get<ConnectorConfig[]>('/api/connector-configs'),

  get: (id: string): Promise<ConnectorConfig | null> =>
    api.get<ConnectorConfig>(`/api/connector-configs/${id}`),

  create: (data: {
    name: string;
    connectorPath: string;
    configJson: Record<string, unknown>;
  }): Promise<ConnectorConfig | null> => api.post<ConnectorConfig>('/api/connector-configs', data),

  update: (
    id: string,
    data: {
      name: string;
      connectorPath: string;
      configJson: Record<string, unknown>;
    },
  ): Promise<ConnectorConfig | null> =>
    api.put<ConnectorConfig>(`/api/connector-configs/${id}`, data),

  delete: (id: string): Promise<unknown> => api.delete(`/api/connector-configs/${id}`),
};

// --- Scheduler Job API ---

export const schedulerJobApi = {
  list: (): Promise<SchedulerJob[] | null> => api.get<SchedulerJob[]>('/api/scheduler/jobs'),

  get: (id: string): Promise<SchedulerJob | null> =>
    api.get<SchedulerJob>(`/api/scheduler/jobs/${id}`),

  create: (data: {
    name: string;
    connectorId: string;
    intervalSecs: number;
  }): Promise<SchedulerJob | null> => api.post<SchedulerJob>('/api/scheduler/jobs', data),

  update: (
    id: string,
    data: { intervalSecs?: number; enabled?: boolean },
  ): Promise<SchedulerJob | null> => api.put<SchedulerJob>(`/api/scheduler/jobs/${id}`, data),

  delete: (id: string): Promise<unknown> => api.delete(`/api/scheduler/jobs/${id}`),

  triggerRun: (id: string): Promise<{ runId: string } | null> =>
    api.post<{ runId: string }>(`/api/scheduler/jobs/${id}/run`),
};

// --- Sync API ---

export const syncApi = {
  listRuns: (): Promise<SyncRun[] | null> => api.get<SyncRun[]>('/api/sync/runs'),

  getRun: (id: string): Promise<SyncRun | null> => api.get<SyncRun>(`/api/sync/runs/${id}`),

  getState: (connectorId: string): Promise<SyncState[] | null> =>
    api.get<SyncState[]>(`/api/sync/state/${connectorId}`),
};
