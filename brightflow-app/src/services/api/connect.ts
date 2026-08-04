/**
 * Connector discovery, presets, runs, and schedules.
 */

import type {
  AvailableConnectorResponse,
  ConnectorConfigResponse,
  EnrichedSyncRun,
  RunTriggerResponse,
  ScheduleResponse,
  SyncRun,
  UnifiedConnector,
} from '@/types/generated';

import { api } from './core';

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
  // Connector-name-based run/schedule — the live path
  runConnector: (name: string): Promise<RunTriggerResponse | null> =>
    api.post<RunTriggerResponse>(`/api/connectors/${encodeURIComponent(name)}/run`),
  scheduleConnector: (name: string, intervalSecs: number): Promise<ScheduleResponse | null> =>
    api.post<ScheduleResponse>(`/api/connectors/${encodeURIComponent(name)}/schedule`, {
      intervalSecs,
    }),
  updateToken: (name: string, token: string): Promise<unknown> =>
    api.put(`/api/connectors/${encodeURIComponent(name)}/token`, { token }),
  deleteSchedule: (id: string): Promise<unknown> => api.delete(`/api/schedules/${id}`),
};
