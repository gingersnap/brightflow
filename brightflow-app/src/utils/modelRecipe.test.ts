/**
 * Unit tests for the recipe bridge: a pivot state captures the pivot chain
 * and a table state the filter/sort/limit chain, both alongside a view
 * snapshot; and every operation kind has words.
 */

import { describe, expect, test } from 'vitest';

import type { Operation } from '@/types/generated';

import { captureRecipe, describeOperation, describeOperations } from './modelRecipe';
import type { PivotSnapshot, QuerySnapshot } from './viewSpec';

const query: QuerySnapshot = {
  filters: [{ column: 'region', id: 'f1', op: 'eq', value: 'EU' }],
  limit: 100,
  sections: {
    filter: { collapsed: false, enabled: true },
    limit: { collapsed: false, enabled: true },
    sort: { collapsed: true, enabled: false },
  },
  sortBy: null,
  sortDescending: false,
};

const emptyPivot: PivotSnapshot = {
  columnFields: [],
  decimalPlaces: 2,
  rowFields: [],
  showColumnTotals: true,
  showConditionalFormatting: false,
  showSubtotals: true,
  valueFields: [],
};

describe('captureRecipe', () => {
  test('a pivot state captures the pivot chain with the view beside it', () => {
    const pivot: PivotSnapshot = {
      ...emptyPivot,
      rowFields: [{ column: 'order_date', datatype: 'String', granularity: 'month', id: 'r1' }],
      valueFields: [{ aggregation: 'sum', column: 'revenue', datatype: 'Float', id: 'v1' }],
    };
    const captured = captureRecipe(query, pivot);
    expect(captured.recipe.version).toBe(1);
    expect(captured.recipe.operations.map((o) => o.type)).toEqual([
      'filter',
      'withColumns',
      'groupBy',
      'limit',
    ]);
    expect(captured.clientSpec.pivot.valueFields[0]?.column).toBe('revenue');
  });

  test('a table state captures filters, sort and limit only when their sections are on', () => {
    const captured = captureRecipe(
      {
        ...query,
        sections: { ...query.sections, sort: { collapsed: false, enabled: true } },
        sortBy: 'revenue',
      },
      emptyPivot,
    );
    expect(captured.recipe.operations.map((o) => o.type)).toEqual(['filter', 'sort', 'limit']);
    const off = captureRecipe(
      { ...query, sections: { ...query.sections, filter: { collapsed: false, enabled: false } } },
      emptyPivot,
    );
    expect(off.recipe.operations.map((o) => o.type)).toEqual(['limit']);
  });
});

describe('describeOperations', () => {
  test('names every kind of step and joins them in order', () => {
    const ops: Operation[] = [
      { column: 'region', op: 'eq', type: 'filter', value: 'EU' },
      {
        columns: [
          { expr: { column: 'order_date', fn: 'period', granularity: 'month' }, name: 'm' },
        ],
        type: 'withColumns',
      },
      { aggs: [{ alias: 'sum', column: 'revenue', function: 'sum' }], by: ['m'], type: 'groupBy' },
      { agg: 'count', columns: 'region', index: ['m'], type: 'pivot', values: 'revenue' },
      { columns: ['m', 'sum'], type: 'select' },
      { by: 'sum', descending: true, type: 'sort' },
      { n: 10, type: 'limit' },
    ];
    expect(ops.map((op) => describeOperation(op))).toEqual([
      'filter region eq "EU"',
      'm = order_date by month',
      'sum of revenue by m',
      'pivot revenue across region by m',
      'keep m, sum',
      'sort by sum descending',
      'top 10',
    ]);
    expect(describeOperations([])).toBe('the whole table');
    expect(describeOperations(ops.slice(0, 2))).toBe(
      'filter region eq "EU" · m = order_date by month',
    );
  });
});
