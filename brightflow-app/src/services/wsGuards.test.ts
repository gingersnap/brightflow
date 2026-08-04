/** Tests for the structural WS frame guards: shape holes must not pass. */

import { describe, expect, test } from 'vitest';

import { isActionBatch, isActionEvent, isInsightsComputed } from './wsGuards';

describe('isActionEvent', () => {
  test('requires a numeric pendingCount and a non-null entry', () => {
    expect(isActionEvent({ entry: { id: 1 }, pendingCount: 0 })).toBe(true);
    expect(isActionEvent({ entry: null, pendingCount: 0 })).toBe(false);
    expect(isActionEvent({ entry: { id: 1 } })).toBe(false);
  });
});

describe('isActionBatch', () => {
  test('requires entries array plus the three scalar fields', () => {
    const ok = { entries: [], pendingCount: 2, succeeded: 1, truncated: false };
    expect(isActionBatch(ok)).toBe(true);
    expect(isActionBatch({ ...ok, entries: 'nope' })).toBe(false);
    expect(isActionBatch({ ...ok, truncated: 'yes' })).toBe(false);
  });
});

describe('isInsightsComputed', () => {
  test('requires sourceId, table, and computedAt', () => {
    const ok = { computedAt: 1, sourceId: 's', table: 't' };
    expect(isInsightsComputed(ok)).toBe(true);
    expect(isInsightsComputed({ ...ok, computedAt: 'now' })).toBe(false);
    expect(isInsightsComputed({ sourceId: 's', table: 't' })).toBe(false);
  });
});
