/**
 * Product analytics: event lists, funnels, retention, and user exploration.
 */

import type {
  EventListRow,
  FunnelResult,
  RetentionResult,
  UserProfile,
  UserTimelineEvent,
} from '@/types/generated';

import { api } from './core';

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
