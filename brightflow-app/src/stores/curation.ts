import { defineStore } from 'pinia';
import { ref } from 'vue';

import { actionsApi } from '@/services/api';
import type { Action, ActionLogEntry, ActionResponse } from '@/types/generated';

/**
 * Curation actions store: dispatches first-class actions (same path an LLM
 * agent uses) with client-generated request ids for idempotency, and keeps
 * the audit feed for the activity drawer.
 */
export const useCurationStore = defineStore('curation', () => {
  const feed = ref<ActionLogEntry[]>([]);
  const feedLoading = ref(false);
  const dispatching = ref(false);
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
    await refreshFeed();
  }

  async function reject(id: number): Promise<void> {
    await actionsApi.reject(id);
    await refreshFeed();
  }

  return {
    approve,
    dispatch,
    dispatching,
    feed,
    feedLoading,
    lastError,
    refreshFeed,
    reject,
    undo,
    version,
  };
});
