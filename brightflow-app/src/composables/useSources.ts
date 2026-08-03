/**
 * The unified source list, on the colada cache.
 *
 * One query key ('unified-sources') shared by every consumer — the sidebar,
 * landing page, tool views, and command palette all read the same cache entry,
 * and mutations that change sources invalidate that one key. The source store
 * holds only the user's selections (period); the list itself lives here.
 */

import { useQuery } from '@pinia/colada';
import { computed } from 'vue';

import { sourceApi } from '@/services/api';
import type { UnifiedSource } from '@/types';

export const UNIFIED_SOURCES_KEY = ['unified-sources'];

export function useSources() {
  const { data, isPending } = useQuery({
    key: UNIFIED_SOURCES_KEY,
    query: async (): Promise<UnifiedSource[]> => (await sourceApi.unifiedList()) ?? [],
  });

  const sources = computed(() => data.value ?? []);

  function sourceById(id: string): UnifiedSource | undefined {
    return sources.value.find((s) => s.id === id);
  }

  return { isPending, sourceById, sources };
}
