import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';

import { insightRunsApi } from '@/services/api';
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

function loadSeenAt(): Record<string, number> {
  try {
    const raw = localStorage.getItem(SEEN_STORAGE_KEY);
    if (raw == null) {
      return {};
    }
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
  const seenAt = ref<Record<string, number>>(loadSeenAt());

  let realtimeInitialized = false;
  const hydratedSources = new Set<string>();

  function persistSeen(): void {
    try {
      localStorage.setItem(SEEN_STORAGE_KEY, JSON.stringify(seenAt.value));
    } catch {
      // Storage full/blocked — the badge just resets next reload.
    }
  }

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

    // Reconnect: refetch the latest runs for every source we've hydrated —
    // Frames pushed while the socket was down are gone for good.
    watch(
      () => connection.isConnected,
      (up) => {
        if (up) {
          for (const sourceId of hydratedSources) {
            void hydrate(sourceId);
          }
        }
      },
    );
  }

  /** REST hydration for one source (on view mount and reconnect). */
  async function hydrate(sourceId: string): Promise<void> {
    hydratedSources.add(sourceId);
    const runs = await insightRunsApi.latest(sourceId);
    for (const run of runs ?? []) {
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

  const totalUnseen = computed(() => {
    let count = 0;
    for (const [key, run] of latestByTable.value) {
      if (run.newFindingCount > 0 && run.computedAt > (seenAt.value[key] ?? 0)) {
        count += run.newFindingCount;
      }
    }
    return count;
  });

  function markSeen(sourceId: string, table: string): void {
    seenAt.value[tableKey(sourceId, table)] = Math.floor(Date.now() / 1000);
    persistSeen();
  }

  function latestFor(sourceId: string, table: string): InsightRunResponse | undefined {
    return latestByTable.value.get(tableKey(sourceId, table));
  }

  return {
    hydrate,
    initRealtime,
    latestByTable,
    latestFor,
    markSeen,
    seenAt,
    totalUnseen,
    unseenCount,
  };
});
