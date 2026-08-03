/**
 * Unit tests for the optimistic curation overlay: applyPatch/revertPatch must
 * be exact inverses, and both must be idempotent — the WS sync path and the
 * optimistic dispatch path both apply the same patches, so double application
 * has to be harmless.
 *
 * Deliberately out of scope: visibleRoots' tree filtering (needs full
 * AnalysisNode fixtures) and the WS stream itself.
 */

import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, test } from 'vitest';

import { type CurationPatch, useInsightsStore } from './insights';

beforeEach(() => {
  setActivePinia(createPinia());
});

/** Snapshot the overlay as plain comparable data. */
function snapshot(store: ReturnType<typeof useInsightsStore>) {
  const o = store.overlay;
  return {
    dismissed: [...o.dismissed].toSorted(),
    notes: [...o.notes.entries()].toSorted((a, b) => a[0].localeCompare(b[0])),
    pinned: [...o.pinned].toSorted(),
    suppressedDimensions: [...o.suppressedDimensions].toSorted(),
    suppressedMeasures: [...o.suppressedMeasures].toSorted(),
  };
}

const PATCHES: CurationPatch[] = [
  { fingerprint: 'fp-1', kind: 'dismiss' },
  { fingerprint: 'fp-2', kind: 'pin', pinned: true },
  { fingerprint: 'fp-3', kind: 'note', note: 'look into this' },
  { kind: 'suppress', target: 'revenue', targetKind: 'column' },
  { kind: 'suppress', target: 'region', targetKind: 'segment' },
];

describe('curation overlay', () => {
  test('revertPatch is the exact inverse of applyPatch', () => {
    const store = useInsightsStore();
    const before = snapshot(store);

    for (const patch of PATCHES) {
      store.applyPatch(patch);
    }
    expect(snapshot(store)).not.toEqual(before);

    for (const patch of PATCHES) {
      store.revertPatch(patch);
    }
    expect(snapshot(store)).toEqual(before);
  });

  test('apply and revert are idempotent', () => {
    const store = useInsightsStore();

    for (const patch of PATCHES) {
      store.applyPatch(patch);
      store.applyPatch(patch);
    }
    const applied = snapshot(store);
    for (const patch of PATCHES) {
      store.applyPatch(patch);
    }
    expect(snapshot(store)).toEqual(applied);

    for (const patch of PATCHES) {
      store.revertPatch(patch);
      store.revertPatch(patch);
    }
    // Everything is back to empty except the pin case below.
    expect(snapshot(store)).toEqual({
      dismissed: [],
      notes: [],
      pinned: [],
      suppressedDimensions: [],
      suppressedMeasures: [],
    });
  });

  test('reverting an unpin re-pins (the pin patch carries direction)', () => {
    const store = useInsightsStore();
    store.applyPatch({ fingerprint: 'fp', kind: 'pin', pinned: true });
    expect(store.overlay.pinned.has('fp')).toBe(true);

    // Unpin applied, then reverted: the pin must come back.
    const unpin: CurationPatch = { fingerprint: 'fp', kind: 'pin', pinned: false };
    store.applyPatch(unpin);
    expect(store.overlay.pinned.has('fp')).toBe(false);
    store.revertPatch(unpin);
    expect(store.overlay.pinned.has('fp')).toBe(true);
  });
});
