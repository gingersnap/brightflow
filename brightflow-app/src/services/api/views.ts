/**
 * Saved views, read side: every view of a source with its table named.
 * Writes are `save_view`, `rename_view` and `delete_view` on the action bus.
 */

import type { SavedViewResponse } from '@/types/generated';

import { api } from './core';

export const viewsApi = {
  list: (sourceId: string): Promise<SavedViewResponse[] | null> =>
    api.get<SavedViewResponse[]>(`/api/sources/${encodeURIComponent(sourceId)}/views`),
};
