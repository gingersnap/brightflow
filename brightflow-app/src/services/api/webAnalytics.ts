/**
 * Web-analytics dashboard reads (stats, timeseries, breakdowns).
 */

import type { BreakdownRow, DashboardStats, TimeseriesPoint } from '@/types/generated';

import { api } from './core';

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
