/**
 * Enrichment functions (versioned derived columns) and the LLM provider
 * settings they run against.
 */

import type {
  EnrichEstimate,
  EnrichFunction,
  EnrichRun,
  FunctionVersion,
  RunScope,
  SampleRunResult,
} from '@/types/enrichment';
import type {
  LlmProviderResponse,
  LlmTestResponse,
  UpsertLlmProviderRequest,
} from '@/types/generated';

import { api } from './core';

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
