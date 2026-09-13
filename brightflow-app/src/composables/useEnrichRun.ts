/**
 * Enrichment run lifecycle: start or resume, follow, cancel.
 *
 * Resumes from the function's `activeRunId` on mount, so a reload during a long
 * run reattaches to it instead of appearing to have lost it. Progress arrives
 * as pushed `job` frames for the run's id; each one is followed by one fetch
 * of the full run row, which carries the token counts the frame does not.
 */

import { onScopeDispose, ref } from 'vue';

import { enrichFnApi } from '@/services/api';
import { isJobEvent } from '@/services/wsGuards';
import { useConnectionStore } from '@/stores/connection';
import type { EnrichRun, RunScope } from '@/types/enrichment';

/**
 * Full/incremental run lifecycle: start (or resume a run found on mount via
 * the function's activeRunId), follow its pushed frames, cancel.
 */
export function useEnrichRun(onFinished?: (run: EnrichRun) => void) {
  const run = ref<EnrichRun | null>(null);
  const errorMessage = ref<string | null>(null);
  const connection = useConnectionStore();

  /** True while pushed frames for the current run are acted on. */
  let following = false;

  async function refresh(): Promise<void> {
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

  const stopJobEvents = connection.onMessage('job', (payload) => {
    if (
      following &&
      isJobEvent(payload) &&
      payload.job.kind === 'enrichment_run' &&
      payload.job.id === run.value?.id
    ) {
      void refresh();
    }
  });
  onScopeDispose(stopJobEvents);

  function beginPolling(): void {
    following = true;
  }
  function stopPolling(): void {
    following = false;
  }

  async function start(functionId: string, scope: RunScope): Promise<void> {
    errorMessage.value = null;
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
    } catch (error) {
      errorMessage.value = error instanceof Error ? error.message : 'Failed to start run';
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
    errorMessage.value = null;
  }

  return { cancel, clear, errorMessage, resume, run, start };
}
