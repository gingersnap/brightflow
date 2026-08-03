/**
 * Unread-insights tracking.
 *
 * "Seen" is deliberately client-side and persisted to localStorage — the server
 * never tracks per-user read state, so this is a local convenience rather than
 * an account-level fact, and clearing site data resets it.
 */

import { useLocalStorage } from '@vueuse/core';
import { defineStore } from 'pinia';
import { ref } from 'vue';

import { useConnectionStore } from '@/stores/connection';
import type { InsightRunResponse, InsightsComputedPayload } from '@/types/generated';

/** Client-side "seen" memory survives reloads; the server never tracks it. */
const SEEN_STORAGE_KEY = 'brightflow.insights.seenAt';

/** Structural guard for a pushed `insightsComputed` frame. */
function isInsightsComputed(
  m: Record<string, unknown>,
): m is Record<string, unknown> & InsightsComputedPayload {
  return (
    typeof m['sourceId'] === 'string' &&
    typeof m['table'] === 'string' &&
    typeof m['computedAt'] === 'number'
  );
}

/** Parse + validate the stored seen-map; anything malformed becomes {}. */
function readSeenAt(raw: string): Record<string, number> {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (parsed == null || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return {};
    }
    const out: Record<string, number> = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (typeof value === 'number') {
        out[key] = value;
      }
    }
    return out;
  } catch {
    return {};
  }
}

function tableKey(sourceId: string, table: string): string {
  return `${sourceId}|${table}`;
}

/**
 * Latest insights computation per table + which the user has already looked
 * at. Powers the sidebar "new findings" badge: post-sync auto-runs push
 * `insightsComputed` frames, this store compares them to the localStorage
 * seen-marks, and `unseenCount` aggregates per source.
 */
export const useInsightsActivityStore = defineStore('insightsActivity', () => {
  /** Latest run per `sourceId|table`. */
  const latestByTable = ref<Map<string, InsightRunResponse>>(new Map());
  const seenAt = useLocalStorage<Record<string, number>>(
    SEEN_STORAGE_KEY,
    {},
    {
      serializer: { read: readSeenAt, write: JSON.stringify },
    },
  );

  let realtimeInitialized = false;

  function upsertRun(run: InsightRunResponse): void {
    const key = tableKey(run.sourceId, run.table);
    const existing = latestByTable.value.get(key);
    if (existing != null && existing.computedAt > run.computedAt) {
      return;
    }
    latestByTable.value.set(key, run);
  }

  function initRealtime(): void {
    if (realtimeInitialized) {
      return;
    }
    realtimeInitialized = true;
    const connection = useConnectionStore();

    connection.onMessage('insightsComputed', (payload) => {
      if (!isInsightsComputed(payload)) {
        return;
      }
      upsertRun({
        computedAt: payload.computedAt,
        executionTimeMs: 0,
        findingCount: payload.findingCount,
        id: payload.runId,
        newFindingCount: payload.newFindingCount,
        reportType: payload.reportType,
        sourceId: payload.sourceId,
        table: payload.table,
        ...(payload.topSummary == null ? {} : { topSummary: payload.topSummary }),
        triggeredBy: payload.triggeredBy,
      });
    });
  }

  /**
   * Merge REST-fetched runs into the push-fed state. The fetch itself lives
   * in `useInsightsActivityFeed` (colada); this store only reconciles, so a
   * stale fetch can never clobber a newer push.
   */
  function ingestRuns(runs: InsightRunResponse[]): void {
    for (const run of runs) {
      upsertRun(run);
    }
  }

  /** New-findings badge count for a source: tables with unseen runs. */
  function unseenCount(sourceId: string): number {
    let count = 0;
    for (const [key, run] of latestByTable.value) {
      const relevant = run.sourceId === sourceId && run.newFindingCount > 0;
      if (relevant && run.computedAt > (seenAt.value[key] ?? 0)) {
        count += run.newFindingCount;
      }
    }
    return count;
  }

  function markSeen(sourceId: string, table: string): void {
    seenAt.value[tableKey(sourceId, table)] = Math.floor(Date.now() / 1000);
  }

  return {
    ingestRuns,
    initRealtime,
    markSeen,
    unseenCount,
  };
});
