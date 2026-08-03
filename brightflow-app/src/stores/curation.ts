/**
 * Pending and applied curation actions, kept in sync with server-pushed events.
 *
 * Dispatch here is not optimistic: a failure sets `lastError` and returns null
 * rather than undoing anything, because this store does not own the overlay an
 * optimistic update would have to roll back. Callers that show instant feedback
 * apply and revert their own patch around this — see `useInsightActions`.
 */

import { defineStore } from 'pinia';
import { ref, watch } from 'vue';

import { actionsApi } from '@/services/api';
import { useConnectionStore } from '@/stores/connection';
import type {
  Action,
  ActionBatchPayload,
  ActionEventPayload,
  ActionLogEntry,
  ActionResponse,
  BulkApproveResponse,
} from '@/types/generated';

/** Feed length the server returns and the client keeps. */
const FEED_LIMIT = 100;

/** Structural guard for a pushed `actionEvent` frame. */
function isActionEvent(
  m: Record<string, unknown>,
): m is Record<string, unknown> & ActionEventPayload {
  return (
    typeof m['pendingCount'] === 'number' && typeof m['entry'] === 'object' && m['entry'] != null
  );
}

/** Structural guard for a pushed `actionBatch` frame. */
function isActionBatch(
  m: Record<string, unknown>,
): m is Record<string, unknown> & ActionBatchPayload {
  return (
    Array.isArray(m['entries']) &&
    typeof m['truncated'] === 'boolean' &&
    typeof m['succeeded'] === 'number' &&
    typeof m['pendingCount'] === 'number'
  );
}

/**
 * Curation actions store: dispatches first-class actions (same path an LLM
 * agent uses) with client-generated request ids for idempotency, and keeps
 * the audit feed for the activity drawer.
 *
 * The feed is push-driven: `initRealtime()` subscribes to typed WS events and
 * upserts entries as they happen. REST responses are control flow only — they
 * never mutate the feed, so the REST+WS double update stays idempotent.
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

  let realtimeInitialized = false;

  /** Replace an entry by id, or insert it keeping (createdAt, id) desc order. */
  function upsertEntry(entry: ActionLogEntry): void {
    const existing = feed.value.findIndex((e) => e.id === entry.id);
    if (existing !== -1) {
      feed.value.splice(existing, 1, entry);
      return;
    }
    const insertAt = feed.value.findIndex(
      (e) => e.createdAt < entry.createdAt || (e.createdAt === entry.createdAt && e.id < entry.id),
    );
    if (insertAt === -1) {
      feed.value.push(entry);
    } else {
      feed.value.splice(insertAt, 0, entry);
    }
    if (feed.value.length > FEED_LIMIT) {
      feed.value.length = FEED_LIMIT;
    }
  }

  /**
   * Subscribe to the WS curation stream. Idempotent — the first caller wins.
   *
   * `version` bumps only on data-changing transitions (applied/undone): that
   * is what topic/insight views key their refetches on, and a proposed or
   * rejected action changes nothing they render.
   */
  function initRealtime(): void {
    if (realtimeInitialized) {
      return;
    }
    realtimeInitialized = true;
    const connection = useConnectionStore();

    connection.onMessage('actionEvent', (payload) => {
      if (!isActionEvent(payload)) {
        return;
      }
      upsertEntry(payload.entry);
      pendingCount.value = payload.pendingCount;
      if (payload.entry.status === 'applied' || payload.entry.status === 'undone') {
        version.value += 1;
      }
    });

    connection.onMessage('actionBatch', (payload) => {
      if (!isActionBatch(payload)) {
        return;
      }
      if (payload.truncated) {
        void refreshFeed();
      } else {
        for (const entry of payload.entries) {
          upsertEntry(entry);
        }
      }
      pendingCount.value = payload.pendingCount;
      if (payload.succeeded > 0) {
        version.value += 1;
      }
    });

    connection.onMessage('actionResync', () => {
      void refreshFeed();
      void refreshPendingCount();
    });

    // Reconnect: v1 has no event cursor, so a fresh connection refetches.
    watch(
      () => connection.isConnected,
      (up) => {
        if (up) {
          void refreshFeed();
          void refreshPendingCount();
        }
      },
    );
  }

  /**
   * Refetch fallback for when the WS stream is down — otherwise the pushed
   * events keep the feed current and REST mutations don't refetch anything.
   */
  function offlineRefresh(dataChanged: boolean): void {
    const connection = useConnectionStore();
    if (connection.isConnected) {
      return;
    }
    if (dataChanged) {
      version.value += 1;
    }
    void refreshFeed();
    void refreshPendingCount();
  }

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
        offlineRefresh(false);
      } else {
        offlineRefresh(true);
      }
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
      feed.value = (await actionsApi.feed(FEED_LIMIT)) ?? [];
    } finally {
      feedLoading.value = false;
    }
  }

  async function undo(id: number): Promise<void> {
    await actionsApi.undo(id);
    offlineRefresh(true);
  }

  async function approve(id: number): Promise<void> {
    await actionsApi.approve(id);
    offlineRefresh(true);
  }

  /**
   * Approve every pending proposal in one server-side sweep.
   *
   * Deliberately NOT a client-side loop over `approve()`: an agent labelling run
   * proposes one action per ticket, so a 1–2k-row seed would mean 1–2k requests.
   * The server applies them oldest-first (proposals have ordering dependencies),
   * reports what failed, and pushes one batch event over the WS stream.
   */
  async function approveAll(): Promise<BulkApproveResponse | null> {
    approvingAll.value = true;
    lastError.value = null;
    try {
      const result = await actionsApi.approveAll();
      if (result != null && result.failed > 0) {
        const first = result.failures[0];
        const example = first == null ? '' : ` — e.g. ${first.actionKind}: ${first.error}`;
        lastError.value = `${result.failed} of ${result.total} could not be applied${example}`;
      }
      offlineRefresh(true);
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
    offlineRefresh(false);
  }

  return {
    approve,
    approveAll,
    approvingAll,
    dispatch,
    dispatching,
    feed,
    feedLoading,
    initRealtime,
    lastError,
    pendingCount,
    refreshFeed,
    refreshPendingCount,
    reject,
    undo,
    version,
  };
});
