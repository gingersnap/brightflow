/**
 * Unit tests for the query store's derived state (`operations`).
 *
 * Scope is the pure computed layer: state goes in via direct ref assignment,
 * operations come out. Deliberately out of scope — the WebSocket round-trip
 * (`useWsQuery`) and whether the backend accepts the emitted operation
 * shapes. Those are integration concerns.
 */

import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, test } from 'vitest';

import type { Filter } from '@/types';

import { filterOperations, useQueryStore } from './query';

/*
 * Filters are pushed directly rather than via addFilter() so ids are
 * deterministic — addFilter() mints a crypto.randomUUID().
 */
function filter(overrides: Partial<Filter> & Pick<Filter, 'id'>): Filter {
  return { column: 'amount', op: 'eq', value: 1, ...overrides };
}

beforeEach(() => {
  setActivePinia(createPinia());
});

describe('operations', () => {
  test('a fresh store already emits the default limit', () => {
    /* Filter and limit sections default to enabled, and limit defaults to 100. */
    const store = useQueryStore();
    expect(store.operations).toEqual([{ n: 100, type: 'limit' }]);
  });

  test('emits nothing when every section is disabled', () => {
    const store = useQueryStore();
    store.filters.push(filter({ id: 'f1' }));
    store.sections.filter.enabled = false;
    store.sections.limit.enabled = false;

    expect(store.operations).toEqual([]);
  });

  test('emits sections in fixed order regardless of when they were enabled', () => {
    const store = useQueryStore();
    store.limit = 50;
    store.sortBy = 'amount';
    store.sections.sort.enabled = true;
    store.filters.push(filter({ id: 'f1' }));

    expect(store.operations.map((op) => op.type)).toEqual(['filter', 'sort', 'limit']);
  });

  test('drops filters with no column or no operator', () => {
    const store = useQueryStore();
    store.sections.limit.enabled = false;
    store.filters.push(
      filter({ column: null, id: 'f1' }),
      filter({ id: 'f2', op: '' }),
      filter({ column: 'region', id: 'f3', op: 'eq', value: 'eu' }),
    );

    expect(store.operations).toEqual([{ column: 'region', op: 'eq', type: 'filter', value: 'eu' }]);
  });

  test('null-check operators force the value to null', () => {
    const store = useQueryStore();
    store.sections.limit.enabled = false;
    store.filters.push(
      filter({ id: 'f1', op: 'isNull', value: 'ignored' }),
      filter({ id: 'f2', op: 'isNotNull', value: 'ignored' }),
    );

    expect(store.operations).toEqual([
      { column: 'amount', op: 'isNull', type: 'filter', value: null },
      { column: 'amount', op: 'isNotNull', type: 'filter', value: null },
    ]);
  });

  test('sort emits only when a sort column is chosen', () => {
    const store = useQueryStore();
    store.sections.limit.enabled = false;
    store.sections.sort.enabled = true;
    expect(store.operations).toEqual([]);

    store.sortBy = 'amount';
    store.sortDescending = true;
    expect(store.operations).toEqual([{ by: 'amount', descending: true, type: 'sort' }]);
  });

  test('limit emits only for positive values', () => {
    const store = useQueryStore();
    store.limit = 0;
    expect(store.operations).toEqual([]);

    store.limit = 25;
    expect(store.operations).toEqual([{ n: 25, type: 'limit' }]);
  });
});

describe('disabled sections', () => {
  test('a disabled section keeps its raw state but emits nothing', () => {
    const store = useQueryStore();
    store.sections.filter.enabled = false;
    store.filters.push(filter({ id: 'f1' }));

    expect(store.filters).toHaveLength(1);
    expect(store.operations).toEqual([{ n: 100, type: 'limit' }]);
  });
});

describe('filterOperations', () => {
  test('skips incomplete filters and nulls the value for null-check ops', () => {
    const filters: Filter[] = [
      filter({ id: 'f1', op: 'eq', value: 42 }),
      { column: null, id: 'f2', op: 'eq', value: 'ignored' },
      filter({ id: 'f3', op: 'isNull', value: 'stale' }),
    ];
    expect(filterOperations(filters)).toEqual([
      { column: 'amount', op: 'eq', type: 'filter', value: 42 },
      { column: 'amount', op: 'isNull', type: 'filter', value: null },
    ]);
  });
});
