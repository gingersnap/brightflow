import { useCurationStore } from '@/stores/curation';
import { type CurationPatch, useInsightsStore } from '@/stores/insights';
import type { Action, DismissReason } from '@/types/generated';

export interface InsightActionFeedback {
  /** Optimistic overlay change; reverted if the dispatch fails. */
  patch?: CurationPatch;
  title: string;
  description?: string;
}

export interface InsightActionScope {
  sourceId: string;
  table: string;
}

/** The overlay patch equivalent to an action, if it has one. */
export function patchFromAction(action: Action): CurationPatch | null {
  if (action.kind === 'dismiss_insight') {
    return { fingerprint: action.fingerprint, kind: 'dismiss' };
  }
  if (action.kind === 'pin_insight') {
    return { fingerprint: action.fingerprint, kind: 'pin', pinned: action.pinned };
  }
  if (action.kind === 'annotate_insight') {
    return { fingerprint: action.fingerprint, kind: 'note', note: action.note };
  }
  if (action.kind === 'suppress_target') {
    return {
      kind: 'suppress',
      target: action.target,
      targetKind: action.target_kind === 'column' ? 'column' : 'segment',
    };
  }
  return null;
}

/**
 * Curation dispatch with instant feedback: the overlay patch applies before
 * the request leaves, the toast confirms (with inline Undo), and a failed or
 * rejected dispatch reverts the patch. The WS `actionEvent` stream applies the
 * same patches idempotently, so no sequencing bookkeeping is needed between
 * the optimistic path and the push path.
 */
export function useInsightActions(): {
  dispatchWithFeedback: (action: Action, feedback: InsightActionFeedback) => Promise<boolean>;
  dismiss: (scope: InsightActionScope, fingerprint: string, reason: DismissReason) => Promise<void>;
  pin: (scope: InsightActionScope, fingerprint: string) => Promise<void>;
  annotate: (scope: InsightActionScope, fingerprint: string, note: string) => Promise<void>;
  suppressSegment: (scope: InsightActionScope, target: string) => Promise<void>;
} {
  const curation = useCurationStore();
  const insightsStore = useInsightsStore();
  const toast = useToast();

  async function dispatchWithFeedback(
    action: Action,
    feedback: InsightActionFeedback,
  ): Promise<boolean> {
    const { patch } = feedback;
    if (patch) {
      insightsStore.applyPatch(patch);
    }
    const response = await curation.dispatch(action);
    if (response == null || response.status === 'failed') {
      if (patch) {
        insightsStore.revertPatch(patch);
      }
      toast.add({
        color: 'error',
        description: curation.lastError ?? 'Unknown error',
        title: 'Action failed',
      });
      return false;
    }
    if (response.status === 'proposed') {
      toast.add({
        color: 'info',
        description: 'Recorded as a proposal — approve it from the Activity feed',
        title: feedback.title,
      });
      return true;
    }
    const logId = Number(response.logId);
    toast.add({
      actions: [
        {
          color: 'neutral',
          label: 'Undo',
          onClick: () => {
            void curation.undo(logId).then(() => {
              // Idempotent with the WS `undone` event for this entry.
              if (patch) {
                insightsStore.revertPatch(patch);
              }
            });
          },
          size: 'xs',
          variant: 'outline',
        },
      ],
      color: 'success',
      title: feedback.title,
      ...(feedback.description == null ? {} : { description: feedback.description }),
    });
    return true;
  }

  async function dismiss(
    scope: InsightActionScope,
    fingerprint: string,
    reason: DismissReason,
  ): Promise<void> {
    await dispatchWithFeedback(
      {
        fingerprint,
        kind: 'dismiss_insight',
        reason,
        source_id: scope.sourceId,
        table: scope.table,
      },
      {
        description: 'It will not come back in future runs',
        patch: { fingerprint, kind: 'dismiss' },
        title: 'Insight dismissed',
      },
    );
  }

  async function pin(scope: InsightActionScope, fingerprint: string): Promise<void> {
    await dispatchWithFeedback(
      {
        fingerprint,
        kind: 'pin_insight',
        pinned: true,
        source_id: scope.sourceId,
        table: scope.table,
      },
      {
        patch: { fingerprint, kind: 'pin', pinned: true },
        title: 'Pinned to top',
      },
    );
  }

  async function annotate(
    scope: InsightActionScope,
    fingerprint: string,
    note: string,
  ): Promise<void> {
    await dispatchWithFeedback(
      {
        fingerprint,
        kind: 'annotate_insight',
        note,
        source_id: scope.sourceId,
        table: scope.table,
      },
      {
        patch: { fingerprint, kind: 'note', note },
        title: 'Note saved',
      },
    );
  }

  async function suppressSegment(scope: InsightActionScope, target: string): Promise<void> {
    // Count before the patch hides them, so the toast can say how many went.
    const affected = insightsStore.countAffectedBySuppress('segment', target);
    await dispatchWithFeedback(
      {
        kind: 'suppress_target',
        source_id: scope.sourceId,
        table: scope.table,
        target,
        target_kind: 'segment',
      },
      {
        description:
          affected > 0
            ? `${affected} finding${affected === 1 ? '' : 's'} hidden`
            : 'Future findings about it will be hidden',
        patch: { kind: 'suppress', target, targetKind: 'segment' },
        title: `Segment “${target}” suppressed`,
      },
    );
  }

  return { annotate, dismiss, dispatchWithFeedback, pin, suppressSegment };
}
