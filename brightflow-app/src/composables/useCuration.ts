/**
 * REST side of the curation surface: dispatch, approve/reject/undo, bulk
 * approve, and the feed/pending-count snapshots (colada queries).
 *
 * Dispatch is not optimistic: a failure sets `lastError` and returns null
 * rather than undoing anything, because nothing here owns the overlay an
 * optimistic update would have to roll back. Callers that show instant
 * feedback apply and revert their own patch around this — see
 * `useInsightActions`. Query results are written into the curation store,
 * which merges them with WS pushes.
 */

import { useQuery } from '@pinia/colada';
import { ref, watch } from 'vue';

import { actionsApi } from '@/services/api';
import { FEED_LIMIT, useCurationStore } from '@/stores/curation';
import type { Action, ActionResponse, BulkApproveResponse } from '@/types/generated';

export function useCuration() {
  const store = useCurationStore();
  store.initRealtime();

  const dispatching = ref(false);
  const approvingAll = ref(false);

  // REST snapshots — refetched whenever the store signals a resync
  // (reconnect, truncated batch, offline mutation).
  const feedQuery = useQuery({
    key: () => ['action-feed', store.resyncTick],
    query: async () => (await actionsApi.feed(FEED_LIMIT)) ?? [],
  });
  watch(feedQuery.data, (entries) => {
    if (entries) {
      store.setFeed(entries);
    }
  });

  const pendingQuery = useQuery({
    key: () => ['action-pending-count', store.resyncTick],
    query: async () => {
      const response = await actionsApi.pendingCount();
      return response?.count ?? 0;
    },
  });
  watch(pendingQuery.data, (count) => {
    if (count != null) {
      store.setPendingCount(count);
    }
  });

  async function dispatch(action: Action): Promise<ActionResponse | null> {
    dispatching.value = true;
    store.lastError = null;
    try {
      const response = await actionsApi.dispatch(action, crypto.randomUUID());
      if (response?.status === 'failed') {
        const result: unknown = response.result;
        const message =
          result != null && typeof result === 'object' && 'error' in result
            ? String((result as Record<string, unknown>)['error'])
            : null;
        store.lastError = message ?? 'Action failed';
        store.offlineRefresh(false);
      } else {
        store.offlineRefresh(true);
      }
      return response;
    } catch (error) {
      store.lastError = error instanceof Error ? error.message : 'Action failed';
      return null;
    } finally {
      dispatching.value = false;
    }
  }

  async function undo(id: number): Promise<void> {
    await actionsApi.undo(id);
    store.offlineRefresh(true);
  }

  async function approve(id: number): Promise<void> {
    await actionsApi.approve(id);
    store.offlineRefresh(true);
  }

  async function reject(id: number): Promise<void> {
    await actionsApi.reject(id);
    store.offlineRefresh(false);
  }

  /**
   * Approve every pending proposal in one server-side sweep.
   *
   * Deliberately NOT a client-side loop over `approve()`: an agent labelling
   * run proposes one action per ticket, so a 1–2k-row seed would mean 1–2k
   * requests. The server applies them oldest-first (proposals have ordering
   * dependencies), reports what failed, and pushes one batch event.
   */
  async function approveAll(): Promise<BulkApproveResponse | null> {
    approvingAll.value = true;
    store.lastError = null;
    try {
      const result = await actionsApi.approveAll();
      if (result != null && result.failed > 0) {
        const first = result.failures[0];
        const example = first == null ? '' : ` — e.g. ${first.actionKind}: ${first.error}`;
        store.lastError = `${result.failed} of ${result.total} could not be applied${example}`;
      }
      store.offlineRefresh(true);
      return result;
    } catch (error) {
      store.lastError = error instanceof Error ? error.message : 'Bulk approve failed';
      return null;
    } finally {
      approvingAll.value = false;
    }
  }

  return {
    approve,
    approveAll,
    approvingAll,
    dispatch,
    dispatching,
    store,
    undo,
    reject,
  };
}
