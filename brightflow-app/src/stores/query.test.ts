/**
 * Unit tests for the query store's derived state (`operations`, `previewTexts`).
 *
 * Scope is the pure computed layer: state goes in via direct ref assignment,
 * operations come out. Deliberately out of scope — the WebSocket round-trip
 * (`useWsQuery`), the components that render these previews, and whether the
 * backend accepts the emitted operation shapes. Those are integration concerns.
 */

import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, test } from 'vitest';

import type { Filter } from '@/types';

import { useQueryStore } from './query';

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
    store.selectedColumns = ['a'];
    store.sections.select.enabled = true;
    store.groupByColumns = ['region'];
    store.sections.groupBy.enabled = true;
    store.filters.push(filter({ id: 'f1' }));

    expect(store.operations.map((op) => op.type)).toEqual([
      'filter',
      'groupBy',
      'select',
      'sort',
      'limit',
    ]);
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

  test('groupBy is skipped when no columns are grouped, even if enabled', () => {
    const store = useQueryStore();
    store.sections.limit.enabled = false;
    store.sections.groupBy.enabled = true;

    expect(store.operations).toEqual([]);
  });

  test('pivot emits only when both values and columns are set', () => {
    const store = useQueryStore();
    store.sections.limit.enabled = false;
    store.sections.pivot.enabled = true;
    store.pivot.index = ['region'];
    store.pivot.values = 'amount';
    expect(store.operations).toEqual([]);

    store.pivot.columns = 'month';
    expect(store.operations).toEqual([
      {
        agg: 'count',
        columns: 'month',
        index: ['region'],
        type: 'pivot',
        values: 'amount',
      },
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

  test('select emits only when columns are chosen', () => {
    const store = useQueryStore();
    store.sections.limit.enabled = false;
    store.sections.select.enabled = true;
    expect(store.operations).toEqual([]);

    store.selectedColumns = ['a', 'b'];
    expect(store.operations).toEqual([{ columns: ['a', 'b'], type: 'select' }]);
  });
});

describe('previewTexts', () => {
  test('pluralizes the filter count', () => {
    const store = useQueryStore();
    expect(store.previewTexts.filter).toBe('No filters');

    store.filters.push(filter({ id: 'f1' }));
    expect(store.previewTexts.filter).toBe('1 filter');

    store.filters.push(filter({ id: 'f2' }));
    expect(store.previewTexts.filter).toBe('2 filters');
  });

  test('pivot reads as unconfigured until values is set', () => {
    const store = useQueryStore();
    expect(store.previewTexts.pivot).toBe('Not configured');

    store.pivot.index = ['region'];
    store.pivot.values = 'amount';
    expect(store.previewTexts.pivot).toBe('1 rows, no columns');

    store.pivot.columns = 'month';
    expect(store.previewTexts.pivot).toBe('1 rows, month columns');
  });

  test('reflects raw state and ignores whether the section is enabled', () => {
    /*
     * Previews intentionally describe what a section *would* do, so a disabled
     * section still shows its configured state.
     */
    const store = useQueryStore();
    store.sections.filter.enabled = false;
    store.filters.push(filter({ id: 'f1' }));

    expect(store.previewTexts.filter).toBe('1 filter');
    expect(store.operations).toEqual([{ n: 100, type: 'limit' }]);
  });

  test('describes the remaining sections from raw state', () => {
    const store = useQueryStore();
    expect(store.previewTexts.groupBy).toBe('Not grouped');
    expect(store.previewTexts.select).toBe('All columns');
    expect(store.previewTexts.sort).toBe('Not sorted');
    expect(store.previewTexts.limit).toBe('100 rows');

    store.groupByColumns = ['region', 'month'];
    store.selectedColumns = ['a'];
    store.sortBy = 'amount';
    store.limit = 1000;

    expect(store.previewTexts.groupBy).toBe('By: region, month');
    expect(store.previewTexts.select).toBe('1 columns');
    expect(store.previewTexts.sort).toBe('amount ASC');
    expect(store.previewTexts.limit).toBe('1,000 rows');
  });
});
