/**
 * Unit tests for the pivot store: new row and column fields start
 * largest-first, `setFieldSort` changes one field without touching the
 * others, value fields carry no sort, a value field's default aggregation
 * follows the column's role before its dtype, and a time column dropped
 * into rows or columns gets the table's granularity and a chronological sort.
 */

import { createPinia, setActivePinia } from 'pinia';
import { beforeEach, describe, expect, test, vi } from 'vitest';

import { DEFAULT_FIELD_SORT } from '@/utils/pivotOrder';

import { useDatasetStore } from './dataset';
import { TIME_FIELD_SORT, usePivotStore } from './pivot';

// The UI store touches `document` on setup; nothing here needs it.
vi.mock('./ui', () => ({ useUiStore: () => ({ resetForNewDataset: (): void => {} }) }));

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

describe('time fields', () => {
  test('a time column gets the table granularity and sorts chronologically', () => {
    const dataset = useDatasetStore();
    dataset.timeGranularity = 'month';
    const store = usePivotStore();
    store.addRowField({ column: 'created_at', dtype: 'string', role: 'time' });
    store.addColumnField({ column: 'ts', dtype: 'datetime' });
    store.addRowField({ column: 'region', dtype: 'string', role: 'dimension' });
    expect(store.rowFields[0]?.granularity).toBe('month');
    expect(store.rowFields[0]?.sort).toEqual(TIME_FIELD_SORT);
    expect(store.columnFields[0]?.granularity).toBe('month');
    expect(store.rowFields[1]?.granularity).toBeUndefined();
    expect(store.rowFields[1]?.sort).toEqual(DEFAULT_FIELD_SORT);
  });

  test('setFieldGranularity changes only time fields', () => {
    const store = usePivotStore();
    store.addRowField({ column: 'created_at', dtype: 'string', role: 'time' });
    store.addRowField({ column: 'region', dtype: 'string', role: 'dimension' });
    const [time, region] = store.rowFields;
    if (time == null || region == null) {
      throw new Error('fields missing');
    }
    store.setFieldGranularity(time.id, 'quarter');
    store.setFieldGranularity(region.id, 'quarter');
    expect(store.rowFields[0]?.granularity).toBe('quarter');
    expect(store.rowFields[1]?.granularity).toBeUndefined();
  });
});
