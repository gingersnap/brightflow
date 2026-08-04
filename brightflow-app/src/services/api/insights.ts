/**
 * Insights: report runs, the novelty-memory history, and the run log behind
 * the badge.
 */

import type { InsightHistoryRow } from '@/types';
import type { InsightRunResponse, InsightsResponse } from '@/types/generated';
import type { EngineConfig } from '@/types/generated/EngineConfig';

import { api } from './core';

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
