/**
 * Text analytics: Text Explorer, the read-only vocabulary views, mention and
 * ticket summaries, and the unresolved-subject queue. Vocabulary writes go
 * through `actionsApi.dispatch` so human edits and agent proposals share one
 * path, one audit log, and one undo.
 */

import type {
  ImportVocabularyResponse,
  MentionSummaryResponse,
  TaxonomyOverview,
  TextExploreRequest,
  TextExploreResponse,
  TicketSummaryResponse,
  UnresolvedSubjectResponse,
  UpdateUnresolvedRequest,
  VocabularyHealthResponse,
} from '@/types/generated';

import { api } from './core';

function tablePath(sourceId: string, table: string): string {
  return `/api/sources/${encodeURIComponent(sourceId)}/tables/${encodeURIComponent(table)}`;
}

// Text Explorer: one POST returns filtered rows + words widget together.
export const textExploreApi = {
  search: (
    sourceId: string,
    table: string,
    body: TextExploreRequest,
  ): Promise<TextExploreResponse | null> =>
    api.post<TextExploreResponse>(`${tablePath(sourceId, table)}/textexplore/search`, body),
};

export const taxonomyApi = {
  overview: (sourceId: string, table: string): Promise<TaxonomyOverview | null> =>
    api.get<TaxonomyOverview>(`${tablePath(sourceId, table)}/taxonomy`),
};

export const vocabularyApi = {
  health: (sourceId: string, table: string): Promise<VocabularyHealthResponse | null> =>
    api.get<VocabularyHealthResponse>(`${tablePath(sourceId, table)}/vocabulary/health`),
  /** CSV text: `kind,parent,name,description,aliases` (aliases `|`-separated). */
  importCsv: (
    sourceId: string,
    table: string,
    csv: string,
  ): Promise<ImportVocabularyResponse | null> =>
    api.post<ImportVocabularyResponse>(`${tablePath(sourceId, table)}/vocabulary/import`, csv),
  unresolved: (
    sourceId: string,
    table: string,
    status: 'open' | 'mapped' | 'ignored' | 'all' = 'open',
  ): Promise<UnresolvedSubjectResponse[] | null> =>
    api.get<UnresolvedSubjectResponse[]>(
      `${tablePath(sourceId, table)}/unresolved-subjects?status=${status}`,
    ),
  updateUnresolved: (
    sourceId: string,
    table: string,
    change: { id: number } & UpdateUnresolvedRequest,
  ): Promise<UnresolvedSubjectResponse | null> => {
    const { id, ...body } = change;
    return api.post<UnresolvedSubjectResponse>(
      `${tablePath(sourceId, table)}/unresolved-subjects/${id}`,
      body,
    );
  },
};

export const mentionsApi = {
  summary: (sourceId: string, table: string): Promise<MentionSummaryResponse | null> =>
    api.get<MentionSummaryResponse>(`${tablePath(sourceId, table)}/mentions/summary`),
};

export const ticketsApi = {
  summary: (sourceId: string, table: string): Promise<TicketSummaryResponse | null> =>
    api.get<TicketSummaryResponse>(`${tablePath(sourceId, table)}/tickets/summary`),
};
