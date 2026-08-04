/**
 * Structural guards for pushed WebSocket frames.
 *
 * One copy so every store that consumes curation-event pushes agrees on what
 * counts as a well-formed frame.
 */

import type {
  ActionBatchPayload,
  ActionEventPayload,
  InsightsComputedPayload,
} from '@/types/generated';

/** Structural guard for a pushed `actionEvent` frame. */
export function isActionEvent(
  m: Record<string, unknown>,
): m is Record<string, unknown> & ActionEventPayload {
  return (
    typeof m['pendingCount'] === 'number' && typeof m['entry'] === 'object' && m['entry'] != null
  );
}

/** Structural guard for a pushed `actionBatch` frame. */
export function isActionBatch(
  m: Record<string, unknown>,
): m is Record<string, unknown> & ActionBatchPayload {
  return (
    Array.isArray(m['entries']) &&
    typeof m['truncated'] === 'boolean' &&
    typeof m['succeeded'] === 'number' &&
    typeof m['pendingCount'] === 'number'
  );
}

/** Structural guard for a pushed `insightsComputed` frame. */
export function isInsightsComputed(
  m: Record<string, unknown>,
): m is Record<string, unknown> & InsightsComputedPayload {
  return (
    typeof m['sourceId'] === 'string' &&
    typeof m['table'] === 'string' &&
    typeof m['computedAt'] === 'number'
  );
}
