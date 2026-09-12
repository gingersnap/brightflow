/**
 * Unit tests for the results store: the two named result sets, the shared
 * loading/error flags, and the setResults defaulting rules.
 */

import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, test } from 'vitest';

import type { ColumnInfo } from '@/types';

import { useResultsStore } from './results';

const COLS: ColumnInfo[] = [
  { datatype: 'Integer', isKpi: null, label: null, name: 'amount', role: null },
];

beforeEach(() => {
  setActivePinia(createPinia());
});

describe('setResults', () => {
  test('routes data to the named result set and leaves the other alone', () => {
    const store = useResultsStore();
    store.setResults('table', { columns: COLS, rows: [[1]] });

    expect(store.table.rows).toEqual([[1]]);
    expect(store.pivot.rows).toEqual([]);
    expect(store.hasTableResults).toBe(true);
    expect(store.hasPivotResults).toBe(false);
    expect(store.hasResults).toBe(true);
  });

  test('defaults rowCount to rows.length and totalRows to rowCount', () => {
    const store = useResultsStore();
    store.setResults('pivot', { columns: COLS, rows: [[1], [2]] });

    expect(store.pivot.rowCount).toBe(2);
    expect(store.pivot.totalRows).toBe(2);
    expect(store.pivot.executionTimeMs).toBeNull();
  });

  test('clears loading and error', () => {
    const store = useResultsStore();
    store.setLoading(true);
    store.setError('boom');
    store.setResults('table', { columns: COLS, rows: [] });

    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
  });
});

describe('loading and error', () => {
  test('setLoading(true) clears a previous error', () => {
    const store = useResultsStore();
    store.setError('boom');
    store.setLoading(true);
    expect(store.error).toBeNull();
    expect(store.loading).toBe(true);
  });

  test('setError accepts strings and message objects', () => {
    const store = useResultsStore();
    store.setError({ message: 'query died' });
    expect(store.error).toBe('query died');
    store.setError({});
    expect(store.error).toBe('Query failed');
  });
});

describe('clear', () => {
  test('empties both result sets and the error', () => {
    const store = useResultsStore();
    store.setResults('table', { columns: COLS, rows: [[1]] });
    store.setResults('pivot', { columns: COLS, rows: [[2]] });
    store.clear();

    expect(store.hasResults).toBe(false);
    expect(store.table.columns).toEqual([]);
    expect(store.pivot.columns).toEqual([]);
    expect(store.error).toBeNull();
  });
});
