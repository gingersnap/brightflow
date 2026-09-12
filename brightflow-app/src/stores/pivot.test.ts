/**
 * Unit tests for the pivot store: new row and column fields start
 * largest-first, `setFieldSort` changes one field without touching the
 * others, value fields carry no sort, and a value field's default
 * aggregation follows the column's role before its dtype.
 */

import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, test } from 'vitest';

import { DEFAULT_FIELD_SORT } from '@/utils/pivotOrder';

import { usePivotStore } from './pivot';

beforeEach(() => {
  setActivePinia(createPinia());
});

describe('field sort', () => {
  test('row and column fields start largest-first; value fields have none', () => {
    const store = usePivotStore();
    store.addRowField({ column: 'category', dtype: 'str' });
    store.addColumnField({ column: 'subcategory', dtype: 'str' });
    store.addValueField({ column: 'id', dtype: 'int' });
    expect(store.rowFields[0]?.sort).toEqual(DEFAULT_FIELD_SORT);
    expect(store.columnFields[0]?.sort).toEqual(DEFAULT_FIELD_SORT);
    expect(store.valueFields[0]?.sort).toBeUndefined();
  });

  test('setFieldSort changes only the addressed field', () => {
    const store = usePivotStore();
    store.addRowField({ column: 'category', dtype: 'str' });
    store.addRowField({ column: 'subcategory', dtype: 'str' });
    const first = store.rowFields[0];
    const second = store.rowFields[1];
    if (first == null || second == null) {
      throw new Error('fields missing');
    }
    store.setFieldSort(second.id, { by: 'label', descending: false });
    expect(store.rowFields[0]?.sort).toEqual(DEFAULT_FIELD_SORT);
    expect(store.rowFields[1]?.sort).toEqual({ by: 'label', descending: false });
    // Unknown id is a no-op.
    store.setFieldSort('nope', { by: 'label', descending: true });
    expect(store.rowFields.map((f) => f.sort?.by)).toEqual(['value', 'label']);
  });
});

describe('default aggregation', () => {
  test('follows the role, then the dtype', () => {
    const store = usePivotStore();
    store.addValueField({ column: 'revenue', dtype: 'f64', role: 'measure' });
    store.addValueField({ column: 'year', dtype: 'i64', role: 'dimension' });
    store.addValueField({ column: 'units', dtype: 'i64' });
    store.addValueField({ column: 'region', dtype: 'string' });
    expect(store.valueFields.map((f) => f.aggregation)).toEqual(['sum', 'count', 'sum', 'count']);
    expect(store.valueFields[0]?.role).toBe('measure');
  });

  test('an explicit aggregation wins over the default', () => {
    const store = usePivotStore();
    store.addValueField({ column: 'revenue', dtype: 'f64', role: 'measure' }, 'avg');
    expect(store.valueFields[0]?.aggregation).toBe('avg');
  });
});
