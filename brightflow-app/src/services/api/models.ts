/**
 * Models, read side: every model of a source with its recipe and last build.
 * Writes are `create_model`, `update_model`, `delete_model` and
 * `rebuild_model` on the action bus.
 */

import type { ModelSummary } from '@/types/generated';

import { api } from './core';

export const modelsApi = {
  list: (sourceId: string): Promise<ModelSummary[] | null> =>
    api.get<ModelSummary[]>(`/api/sources/${encodeURIComponent(sourceId)}/models`),
};
