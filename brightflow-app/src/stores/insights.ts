/**
 * Insights report state: the analysis tree, client-side filters, and the
 * curation overlay.
 *
 * Filtering happens client-side over the already-ranked findings so toggling a
 * filter is instant and never re-runs the analysis. The overlay keeps optimistic
 * curation edits separate from the server's tree, so a failed action reverts
 * without having to refetch the report. The runs themselves are fired by
 * `composables/useInsightsRuns`, which feeds results back in through
 * `beginRun`/`completeRun`/`failRun`.
 */

import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

import { dimensionOf, directionOf, measureOf } from '@/components/insights/nodeMeta';
import type { AnalysisNode, AnalysisTree } from '@/services/api';
import { isActionEvent } from '@/services/wsGuards';
import { useConnectionStore } from '@/stores/connection';
import type { ActionLogEntry, InsightsResponse } from '@/types/generated';

export type ReportType = 'review' | 'trends' | 'drivers';
export type Cadence = 'daily' | 'weekly' | 'monthly';

/** UI filter state — applied client-side over the ranked findings */
export interface InsightFilters {
  /** Empty = all types pass */
  types: Set<string>;
  direction: 'up' | 'down' | 'both';
  /** Null = all measures pass */
  measure: string | null;
  minScore: number;
}

/**
 * One optimistic curation change, applied to the loaded tree the instant the
 * user acts and reverted if the server rejects it. The server converges on the
 * same result at the next run via `apply_curation`, so the overlay never needs
 * to survive a re-run — set-filtering fingerprints that are no longer present
 * is a harmless no-op.
 */
export type CurationPatch =
  | { kind: 'dismiss'; fingerprint: string }
  | { kind: 'pin'; fingerprint: string; pinned: boolean }
  | { kind: 'note'; fingerprint: string; note: string }
  | { kind: 'suppress'; targetKind: 'segment' | 'column'; target: string };

interface CurationOverlay {
  dismissed: Set<string>;
  pinned: Set<string>;
  /** Suppressed `column` targets — matched against `measureOf`. */
  suppressedMeasures: Set<string>;
  /** Suppressed `segment` targets — matched against `dimensionOf`. */
  suppressedDimensions: Set<string>;
  notes: Map<string, string>;
}

function emptyOverlay(): CurationOverlay {
  return {
    dismissed: new Set(),
    notes: new Map(),
    pinned: new Set(),
    suppressedDimensions: new Set(),
    suppressedMeasures: new Set(),
  };
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return v != null && typeof v === 'object';
}

/** Map a WS action-log entry to the equivalent overlay patch, if any. */
function patchFromEntry(entry: ActionLogEntry): CurationPatch | null {
  const params: unknown = entry.params;
  if (!isRecord(params)) {
    return null;
  }
  const p = params;
  const fingerprint = typeof p['fingerprint'] === 'string' ? p['fingerprint'] : '';
  switch (entry.actionKind) {
    case 'dismiss_insight': {
      return fingerprint ? { fingerprint, kind: 'dismiss' } : null;
    }
    case 'pin_insight': {
      return fingerprint ? { fingerprint, kind: 'pin', pinned: p['pinned'] === true } : null;
    }
    case 'annotate_insight': {
      return fingerprint
        ? { fingerprint, kind: 'note', note: typeof p['note'] === 'string' ? p['note'] : '' }
        : null;
    }
    case 'suppress_target': {
      const target = typeof p['target'] === 'string' ? p['target'] : '';
      const targetKind = p['target_kind'] === 'column' ? 'column' : 'segment';
      return target ? { kind: 'suppress', target, targetKind } : null;
    }
    default: {
      return null;
    }
  }
}

export const useInsightsStore = defineStore('insights', () => {
  const selectedSourceId = ref<string | null>(null);
  const selectedTable = ref<string | null>(null);
  const tree = ref<AnalysisTree | null>(null);
  const reportType = ref<ReportType>('review');
  const cadence = ref<Cadence>('weekly');
  const loading = ref(false);
  const error = ref<string | null>(null);
  const executionTimeMs = ref<number | null>(null);
  const nodeCount = ref(0);
  const findingCount = ref(0);
  const firstLevelCount = ref(0);
  const deeperCount = ref(0);
  const totalCandidates = ref(0);
  const showAll = ref(false);

  const filters = ref<InsightFilters>({
    direction: 'both',
    measure: null,
    minScore: 0,
    types: new Set(),
  });

  // ── Optimistic curation overlay ───────────────────────────────────────────

  const overlay = ref<CurationOverlay>(emptyOverlay());
  let curationSyncInitialized = false;

  /** Idempotent: applying the same patch twice is a no-op. */
  function applyPatch(patch: CurationPatch): void {
    const o = overlay.value;
    switch (patch.kind) {
      case 'dismiss': {
        o.dismissed.add(patch.fingerprint);
        break;
      }
      case 'pin': {
        if (patch.pinned) {
          o.pinned.add(patch.fingerprint);
        } else {
          o.pinned.delete(patch.fingerprint);
        }
        break;
      }
      case 'note': {
        o.notes.set(patch.fingerprint, patch.note);
        break;
      }
      case 'suppress': {
        if (patch.targetKind === 'column') {
          o.suppressedMeasures.add(patch.target);
        } else {
          o.suppressedDimensions.add(patch.target);
        }
        break;
      }
    }
  }

  /** Inverse of `applyPatch`; also idempotent. */
  function revertPatch(patch: CurationPatch): void {
    const o = overlay.value;
    switch (patch.kind) {
      case 'dismiss': {
        o.dismissed.delete(patch.fingerprint);
        break;
      }
      case 'pin': {
        if (patch.pinned) {
          o.pinned.delete(patch.fingerprint);
        } else {
          o.pinned.add(patch.fingerprint);
        }
        break;
      }
      case 'note': {
        o.notes.delete(patch.fingerprint);
        break;
      }
      case 'suppress': {
        if (patch.targetKind === 'column') {
          o.suppressedMeasures.delete(patch.target);
        } else {
          o.suppressedDimensions.delete(patch.target);
        }
        break;
      }
    }
  }

  /**
   * Keep the overlay in sync with the WS action stream, so curation from any
   * surface — agent triage, the Activity feed's undo, another tab — moves
   * cards live. Idempotent with the optimistic dispatch path: both sides call
   * the same set operations, so double application is harmless.
   */
  function initCurationSync(): void {
    if (curationSyncInitialized) {
      return;
    }
    curationSyncInitialized = true;
    const connection = useConnectionStore();
    connection.onMessage('actionEvent', (payload) => {
      if (!isActionEvent(payload)) {
        return;
      }
      const patch = patchFromEntry(payload.entry);
      if (patch == null) {
        return;
      }
      if (payload.entry.status === 'applied') {
        applyPatch(patch);
      } else if (payload.entry.status === 'undone') {
        revertPatch(patch);
      }
    });
  }

  /** How many currently visible findings a suppression patch would hide. */
  function countAffectedBySuppress(targetKind: 'segment' | 'column', target: string): number {
    return visibleRoots.value.filter((n) =>
      targetKind === 'column' ? measureOf(n) === target : dimensionOf(n) === target,
    ).length;
  }

  // Returns root nodes filtered + sorted by significance, with the curation
  // Overlay applied (dismissed/suppressed drop out, pinned float first) —
  // Mirroring the server's `apply_curation`.
  const visibleRoots = computed<AnalysisNode[]>(() => {
    if (!tree.value) {
      return [];
    }
    const nodeMap = new Map<number, AnalysisNode>();
    for (const n of tree.value.nodes) {
      nodeMap.set(n.id, n);
    }
    const roots = tree.value.roots
      .map((id) => nodeMap.get(id))
      .filter((n): n is AnalysisNode => n !== undefined);

    const o = overlay.value;
    const isPinned = (n: AnalysisNode): boolean =>
      n.fingerprint !== '' && o.pinned.has(n.fingerprint);

    return roots
      .filter((n) => {
        if (n.fingerprint !== '' && o.dismissed.has(n.fingerprint)) {
          return false;
        }
        if (o.suppressedMeasures.has(measureOf(n))) {
          return false;
        }
        const dim = dimensionOf(n);
        if (dim !== null && o.suppressedDimensions.has(dim)) {
          return false;
        }
        if (filters.value.types.size > 0 && !filters.value.types.has(n.analysis.type)) {
          return false;
        }
        if (n.significance < filters.value.minScore) {
          return false;
        }
        if (filters.value.direction !== 'both') {
          const dir = directionOf(n);
          if (dir !== null && dir !== filters.value.direction) {
            return false;
          }
        }
        if (filters.value.measure !== null) {
          const col = measureOf(n);
          if (col !== filters.value.measure) {
            return false;
          }
        }
        return true;
      })
      .toSorted((a, b) => {
        const pinDelta = Number(isPinned(b)) - Number(isPinned(a));
        if (pinDelta !== 0) {
          return pinDelta;
        }
        // Diversity-selected rank from the engine wins; fall back to score
        if (a.rank != null && b.rank != null) {
          return a.rank - b.rank;
        }
        if (a.rank != null) {
          return -1;
        }
        if (b.rank != null) {
          return 1;
        }
        return b.significance - a.significance;
      });
  });

  // The set of measures referenced anywhere in the tree (for the measure filter dropdown)
  const availableMeasures = computed<string[]>(() => {
    if (!tree.value) {
      return [];
    }
    const set = new Set<string>();
    for (const n of tree.value.nodes) {
      set.add(measureOf(n));
    }
    return [...set].toSorted();
  });

  // The set of analysis types present in the roots (for filter chips)
  const availableTypes = computed<string[]>(() => {
    if (!tree.value) {
      return [];
    }
    const set = new Set<string>();
    for (const id of tree.value.roots) {
      const n = tree.value.nodes.find((x) => x.id === id);
      if (n) {
        set.add(n.analysis.type);
      }
    }
    return [...set].toSorted();
  });

  function selectTable(sourceId: string, name: string): void {
    if (sourceId === selectedSourceId.value && name === selectedTable.value) {
      return;
    }
    selectedSourceId.value = sourceId;
    selectedTable.value = name;
    resetState();
  }

  function resetState(): void {
    tree.value = null;
    error.value = null;
    executionTimeMs.value = null;
    nodeCount.value = 0;
    findingCount.value = 0;
    firstLevelCount.value = 0;
    deeperCount.value = 0;
    totalCandidates.value = 0;
    showAll.value = false;
    filters.value = { direction: 'both', measure: null, minScore: 0, types: new Set() };
    // Deliberately NOT cleared on re-run — the server converges via its own
    // Curation state, and stale entries are harmless set lookups.
    overlay.value = emptyOverlay();
  }

  /**
   * Mark a run of `type` as started. Returns false — and sets `error` — when
   * no table is selected, so the caller can skip the fetch entirely.
   */
  function beginRun(type: ReportType, selectedCadence?: Cadence): boolean {
    if (selectedSourceId.value == null || selectedTable.value == null) {
      error.value = 'No table selected';
      return false;
    }
    loading.value = true;
    error.value = null;
    if (selectedCadence != null) {
      cadence.value = selectedCadence;
    }
    reportType.value = type;
    return true;
  }

  function completeRun(result: InsightsResponse | null): void {
    if (result) {
      tree.value = result.tree;
      executionTimeMs.value = result.executionTimeMs;
      nodeCount.value = result.nodeCount;
      findingCount.value = result.findingCount;
      firstLevelCount.value = result.firstLevelCount;
      deeperCount.value = result.deeperCount;
      totalCandidates.value = result.totalCandidates;
      showAll.value = false;
    }
    loading.value = false;
  }

  function failRun(message: string): void {
    error.value = message;
    tree.value = null;
    loading.value = false;
  }

  function reset(): void {
    selectedSourceId.value = null;
    selectedTable.value = null;
    resetState();
    loading.value = false;
  }

  return {
    applyPatch,
    availableMeasures,
    availableTypes,
    beginRun,
    cadence,
    completeRun,
    countAffectedBySuppress,
    deeperCount,
    error,
    executionTimeMs,
    failRun,
    filters,
    findingCount,
    firstLevelCount,
    initCurationSync,
    loading,
    nodeCount,
    overlay,
    reportType,
    reset,
    revertPatch,
    selectTable,
    selectedSourceId,
    selectedTable,
    showAll,
    totalCandidates,
    tree,
    visibleRoots,
  };
});
