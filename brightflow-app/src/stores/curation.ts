import { defineStore } from 'pinia';
import { ref } from 'vue';

import { actionsApi } from '@/services/api';
import type {
  Action,
  ActionLogEntry,
  ActionResponse,
  BulkApproveResponse,
} from '@/types/generated';

/**
 * Curation actions store: dispatches first-class actions (same path an LLM
 * agent uses) with client-generated request ids for idempotency, and keeps
 * the audit feed for the activity drawer.
 */
export const useCurationStore = defineStore('curation', () => {
  const feed = ref<ActionLogEntry[]>([]);
  const feedLoading = ref(false);
  const dispatching = ref(false);
  const approvingAll = ref(false);
  /** Proposals awaiting review — the true count, not the feed's visible slice. */
  const pendingCount = ref(0);
  const lastError = ref<string | null>(null);
  /** Bumped after every applied action so views can refetch. */
  const version = ref(0);

  async function dispatch(action: Action): Promise<ActionResponse | null> {
    dispatching.value = true;
    lastError.value = null;
    try {
      const response = await actionsApi.dispatch(action, crypto.randomUUID());
      if (response?.status === 'failed') {
        const result: unknown = response.result;
        const message =
          result != null && typeof result === 'object' && 'error' in result
            ? String((result as Record<string, unknown>)['error'])
            : null;
        lastError.value = message ?? 'Action failed';
      } else {
        version.value += 1;
      }
      await refreshFeed();
      return response;
      // oxlint-disable-next-line unicorn/catch-error-name -- error shadows ref
    } catch (err) {
      lastError.value = err instanceof Error ? err.message : 'Action failed';
      return null;
    } finally {
      dispatching.value = false;
    }
  }

  async function refreshFeed(): Promise<void> {
    feedLoading.value = true;
    try {
      feed.value = (await actionsApi.feed(100)) ?? [];
    } finally {
      feedLoading.value = false;
    }
  }

  async function undo(id: number): Promise<void> {
    await actionsApi.undo(id);
    version.value += 1;
    await refreshFeed();
  }

  async function approve(id: number): Promise<void> {
    await actionsApi.approve(id);
    version.value += 1;
    await Promise.all([refreshFeed(), refreshPendingCount()]);
  }

  /**
   * Approve every pending proposal in one server-side sweep.
   *
   * Deliberately NOT a client-side loop over `approve()`: an agent labelling run
   * proposes one action per ticket, so a 1–2k-row seed would mean 1–2k requests
   * — each of which also refetches the whole feed. The server applies them
   * oldest-first (proposals have ordering dependencies) and reports what failed.
   */
  async function approveAll(): Promise<BulkApproveResponse | null> {
    approvingAll.value = true;
    lastError.value = null;
    try {
      const result = await actionsApi.approveAll();
      version.value += 1;
      if (result != null && result.failed > 0) {
        const first = result.failures[0];
        const example = first == null ? '' : ` — e.g. ${first.actionKind}: ${first.error}`;
        lastError.value = `${result.failed} of ${result.total} could not be applied${example}`;
      }
      await Promise.all([refreshFeed(), refreshPendingCount()]);
      return result;
      // oxlint-disable-next-line unicorn/catch-error-name -- error shadows ref
    } catch (err) {
      lastError.value = err instanceof Error ? err.message : 'Bulk approve failed';
      return null;
    } finally {
      approvingAll.value = false;
    }
  }

  /**
   * True pending count. The feed is capped server-side, so counting `feed` would
   * under-report once an agent proposes more than one page of actions.
   */
  async function refreshPendingCount(): Promise<void> {
    const response = await actionsApi.pendingCount();
    pendingCount.value = response?.count ?? 0;
  }

  async function reject(id: number): Promise<void> {
    await actionsApi.reject(id);
    await Promise.all([refreshFeed(), refreshPendingCount()]);
  }

  return {
    approve,
    approveAll,
    approvingAll,
    dispatch,
    dispatching,
    feed,
    feedLoading,
    lastError,
    pendingCount,
    refreshFeed,
    refreshPendingCount,
    reject,
    undo,
    version,
  };
});
