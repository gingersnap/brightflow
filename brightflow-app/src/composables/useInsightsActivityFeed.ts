/**
 * Colada-side half of the unread-insights badge: fetches the latest insight
 * run per table for a set of sources and feeds them into the activity store,
 * which merges them with WebSocket pushes.
 *
 * Refetches when the query socket reconnects — frames pushed while the socket
 * was down are gone for good, so the REST snapshot is the recovery path.
 */

import { useQuery } from '@pinia/colada';
import { computed, watch } from 'vue';

import { insightRunsApi } from '@/services/api';
import { useConnectionStore } from '@/stores/connection';
import { useInsightsActivityStore } from '@/stores/insightsActivity';
import type { UnifiedSource } from '@/types';

export function useInsightsActivityFeed(sources: () => UnifiedSource[]): void {
  const insightsActivity = useInsightsActivityStore();
  const connection = useConnectionStore();

  const sourceIds = computed(() => sources().map((s) => s.id));

  const { data, refetch } = useQuery({
    key: () => ['insight-latest-runs', ...sourceIds.value],
    query: async () => {
      const all = await Promise.all(sourceIds.value.map((id) => insightRunsApi.latest(id)));
      return all.flatMap((runs) => runs ?? []);
    },
    enabled: () => sourceIds.value.length > 0,
  });

  watch(data, (runs) => {
    if (runs) {
      insightsActivity.ingestRuns(runs);
    }
  });

  watch(
    () => connection.isConnected,
    (up) => {
      if (up) {
        void refetch();
      }
    },
  );
}
