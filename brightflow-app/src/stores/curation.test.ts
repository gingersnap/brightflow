/**
 * Unit tests for the curation store's feed-merge logic.
 *
 * Deliberately out of scope: the WS stream and REST calls — `upsertEntry` and
 * the snapshot setters are pure state transitions, which is what a pushed
 * event and a fetched snapshot must agree on.
 */

import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, test } from 'vitest';

import type { ActionLogEntry } from '@/types/generated';

import { FEED_LIMIT, useCurationStore } from './curation';

function entry(overrides: Partial<ActionLogEntry> & { id: number }): ActionLogEntry {
  return {
    actionKind: 'rename_cluster',
    actorType: 'human',
    createdAt: 1000,
    params: {},
    requestId: 'req',
    status: 'applied',
    undoable: false,
    ...overrides,
  };
}

beforeEach(() => {
  setActivePinia(createPinia());
});

describe('upsertEntry', () => {
  test('inserts keeping (createdAt, id) descending order', () => {
    const store = useCurationStore();
    store.upsertEntry(entry({ id: 1, createdAt: 100 }));
    store.upsertEntry(entry({ id: 3, createdAt: 300 }));
    store.upsertEntry(entry({ id: 2, createdAt: 200 }));

    expect(store.feed.map((e) => e.id)).toEqual([3, 2, 1]);
  });

  test('ties on createdAt order by id descending', () => {
    const store = useCurationStore();
    store.upsertEntry(entry({ id: 5, createdAt: 100 }));
    store.upsertEntry(entry({ id: 7, createdAt: 100 }));
    store.upsertEntry(entry({ id: 6, createdAt: 100 }));

    expect(store.feed.map((e) => e.id)).toEqual([7, 6, 5]);
  });

  test('replaces an existing entry in place instead of duplicating', () => {
    const store = useCurationStore();
    store.upsertEntry(entry({ id: 1, createdAt: 100, status: 'proposed' }));
    store.upsertEntry(entry({ id: 1, createdAt: 100, status: 'applied' }));

    expect(store.feed).toHaveLength(1);
    expect(store.feed[0]?.status).toBe('applied');
  });

  test('caps the feed at FEED_LIMIT, dropping the oldest', () => {
    const store = useCurationStore();
    for (let i = 1; i <= FEED_LIMIT + 5; i++) {
      store.upsertEntry(entry({ id: i, createdAt: i }));
    }
    expect(store.feed).toHaveLength(FEED_LIMIT);
    // Newest first; the oldest five fell off the end.
    expect(store.feed[0]?.id).toBe(FEED_LIMIT + 5);
    expect(store.feed.at(-1)?.id).toBe(6);
  });
});
