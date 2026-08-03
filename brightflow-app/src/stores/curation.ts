/**
 * Pending and applied curation actions, kept in sync with server-pushed events.
 *
 * Client state only: this store merges WS pushes into the feed and tracks
 * counts/versions; every REST call (dispatch, approve, feed fetch) lives in
 * `composables/useCuration`, which writes results back in through the setters
 * here. When the socket is down or the server says the stream truncated, the
 * store bumps `resyncTick` and the composable's queries refetch.
 */

import { defineStore } from 'pinia';
import { ref, watch } from 'vue';

import { isActionEvent } from '@/services/wsGuards';
import { useConnectionStore } from '@/stores/connection';
import type { ActionBatchPayload, ActionLogEntry } from '@/types/generated';

/** Feed length the server returns and the client keeps. */
export const FEED_LIMIT = 100;

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

export const useCurationStore = defineStore('curation', () => {
  const feed = ref<ActionLogEntry[]>([]);
  /** Proposals awaiting review — the true count, not the feed's visible slice. */
  const pendingCount = ref(0);
  const lastError = ref<string | null>(null);
  /** Bumped after every applied action so views can refetch. */
  const version = ref(0);
  /** Bumped whenever the REST snapshot must be refetched (resync/reconnect). */
  const resyncTick = ref(0);

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

  /** Replace the whole feed from a REST snapshot (pushes keep it current after). */
  function setFeed(entries: ActionLogEntry[]): void {
    feed.value = entries;
  }

  function setPendingCount(count: number): void {
    pendingCount.value = count;
  }

  /**
   * After a REST mutation while the WS stream is down: the pushed event that
   * would have updated the feed is lost, so force a refetch. No-op while
   * connected — the push keeps the REST+WS double update idempotent.
   */
  function offlineRefresh(dataChanged: boolean): void {
    const connection = useConnectionStore();
    if (connection.isConnected) {
      return;
    }
    if (dataChanged) {
      version.value += 1;
    }
    resyncTick.value += 1;
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
        resyncTick.value += 1;
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
      resyncTick.value += 1;
    });

    // Reconnect: v1 has no event cursor, so a fresh connection refetches.
    watch(
      () => connection.isConnected,
      (up) => {
        if (up) {
          resyncTick.value += 1;
        }
      },
    );
  }

  return {
    feed,
    initRealtime,
    lastError,
    offlineRefresh,
    pendingCount,
    resyncTick,
    setFeed,
    setPendingCount,
    upsertEntry,
    version,
  };
});
