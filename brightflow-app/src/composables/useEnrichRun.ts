import { onBeforeUnmount, ref } from 'vue';

import { enrichFnApi } from '@/services/api';
import type { EnrichRun, RunScope } from '@/types/enrichment';

/**
 * Full/incremental run lifecycle: start (or resume a run found on mount via
 * the function's activeRunId), poll every 2s, cancel. Same cadence as
 * AgentActions.vue.
 */
export function useEnrichRun(onFinished?: (run: EnrichRun) => void) {
  const run = ref<EnrichRun | null>(null);
  const error = ref<string | null>(null);
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  onBeforeUnmount(stopPolling);

  function stopPolling(): void {
    if (pollTimer != null) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  async function poll(): Promise<void> {
    if (run.value == null) {
      return;
    }
    const updated = await enrichFnApi.getRun(run.value.id).catch(() => null);
    if (updated == null) {
      return;
    }
    run.value = updated;
    if (updated.status !== 'running') {
      stopPolling();
      onFinished?.(updated);
    }
  }

  function beginPolling(): void {
    stopPolling();
    pollTimer = setInterval(() => void poll(), 2000);
  }

  async function start(functionId: string, scope: RunScope): Promise<void> {
    error.value = null;
    try {
      const started = await enrichFnApi.startRun(functionId, scope);
      if (started == null) {
        throw new Error('Failed to start run');
      }
      const initial = await enrichFnApi.getRun(started.runId);
      if (initial != null) {
        run.value = initial;
      }
      beginPolling();
      // oxlint-disable-next-line unicorn/catch-error-name -- error shadows ref
    } catch (err) {
      error.value = err instanceof Error ? err.message : 'Failed to start run';
    }
  }

  /** Re-attach to a run that was already active when the editor mounted. */
  async function resume(runId: string): Promise<void> {
    const existing = await enrichFnApi.getRun(runId).catch(() => null);
    if (existing == null) {
      return;
    }
    run.value = existing;
    if (existing.status === 'running') {
      beginPolling();
    }
  }

  async function cancel(): Promise<void> {
    if (run.value == null) {
      return;
    }
    const cancelled = await enrichFnApi.cancelRun(run.value.id).catch(() => null);
    if (cancelled != null) {
      run.value = cancelled;
    }
    stopPolling();
  }

  function clear(): void {
    stopPolling();
    run.value = null;
    error.value = null;
  }

  return { cancel, clear, error, resume, run, start };
}
