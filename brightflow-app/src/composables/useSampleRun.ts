import { computed, ref } from 'vue';

import { enrichFnApi } from '@/services/api';
import type { SampleCell, SampleRunResult } from '@/types/enrichment';

/**
 * The sample→refine→scale loop: test N rows against the current draft,
 * snapshot the previous results before each run so "Compare with previous"
 * can highlight what a prompt edit changed. Results survive draft edits.
 */
export function useSampleRun(functionId: () => string | null) {
  const rows = ref<SampleCell[]>([]);
  const previousRows = ref<SampleCell[]>([]);
  const result = ref<SampleRunResult | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const runCount = ref(0);
  const limit = ref(10);

  /** Cached/new badges are only meaningful after the first run. */
  const showCacheBadges = computed(() => runCount.value > 1);

  const previousByKey = computed(() => {
    const map = new Map<string, SampleCell>();
    for (const cell of previousRows.value) {
      map.set(cell.rowKey, cell);
    }
    return map;
  });

  /** Row keys whose value changed vs the previous run. */
  const changedKeys = computed(() => {
    const changed = new Set<string>();
    for (const cell of rows.value) {
      const prev = previousByKey.value.get(cell.rowKey);
      if (prev != null && JSON.stringify(prev.value) !== JSON.stringify(cell.value)) {
        changed.add(cell.rowKey);
      }
    }
    return changed;
  });

  async function run(config: unknown, requestedLimit?: number): Promise<void> {
    const id = functionId();
    if (id == null || loading.value) {
      return;
    }
    if (requestedLimit != null) {
      limit.value = requestedLimit;
    }
    loading.value = true;
    error.value = null;
    if (rows.value.length > 0) {
      previousRows.value = rows.value;
    }
    try {
      const body: { limit: number; config?: unknown } = { limit: limit.value };
      if (config != null) {
        body.config = config;
      }
      const response = await enrichFnApi.sampleRun(id, body);
      if (response == null) {
        throw new Error('Empty sample-run response');
      }
      result.value = response;
      rows.value = response.rows;
      runCount.value += 1;
      // oxlint-disable-next-line unicorn/catch-error-name -- error shadows ref
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Sample run failed';
    } finally {
      loading.value = false;
    }
  }

  /** Widen to 100 rows — the first N come back as cache hits. */
  async function widen(config: unknown): Promise<void> {
    await run(config, 100);
  }

  function reset(): void {
    rows.value = [];
    previousRows.value = [];
    result.value = null;
    error.value = null;
    runCount.value = 0;
    limit.value = 10;
  }

  return {
    changedKeys,
    error,
    limit,
    loading,
    previousByKey,
    previousRows,
    reset,
    result,
    rows,
    run,
    runCount,
    showCacheBadges,
    widen,
  };
}
