/**
 * Structural guards for pushed WebSocket frames.
 *
 * One copy so every store that consumes curation-event pushes agrees on what
 * counts as a well-formed frame.
 */

import type { ActionEventPayload } from '@/types/generated';

/** Structural guard for a pushed `actionEvent` frame. */
export function isActionEvent(
  m: Record<string, unknown>,
): m is Record<string, unknown> & ActionEventPayload {
  return (
    typeof m['pendingCount'] === 'number' && typeof m['entry'] === 'object' && m['entry'] != null
  );
}
