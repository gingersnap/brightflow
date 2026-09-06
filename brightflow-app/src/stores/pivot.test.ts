/**
 * Unit tests for the pivot store's field sort: new row and column fields
 * start largest-first, `setFieldSort` changes one field without touching
 * the others, and value fields carry no sort.
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
    store.addRowField('category', 'str');
    store.addColumnField('subcategory', 'str');
    store.addValueField('id', 'int');
    expect(store.rowFields[0]?.sort).toEqual(DEFAULT_FIELD_SORT);
    expect(store.columnFields[0]?.sort).toEqual(DEFAULT_FIELD_SORT);
    expect(store.valueFields[0]?.sort).toBeUndefined();
  });

  test('setFieldSort changes only the addressed field', () => {
    const store = usePivotStore();
    store.addRowField('category', 'str');
    store.addRowField('subcategory', 'str');
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
