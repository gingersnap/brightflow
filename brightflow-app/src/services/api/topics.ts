/**
 * Topics, text exploration, and the read-only taxonomy views. Taxonomy
 * writes go through `actionsApi.dispatch` so human edits and agent proposals
 * share one path, one audit log, and one undo.
 */

import type {
  ClusterDetail,
  CurationQueue,
  EnrichmentSettingsResponse,
  TaxonomyOverview,
  TextExploreRequest,
  TextExploreResponse,
  TopicsOverview,
  UpdateEnrichmentSettingsRequest,
} from '@/types/generated';

import { api } from './core';

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
