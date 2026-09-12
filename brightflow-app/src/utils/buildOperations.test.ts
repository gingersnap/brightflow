/**
 * Unit tests for the pivot operation builder: the group-by and pivot shapes,
 * the period bucketing that precedes them, and that fields without a
 * granularity emit no derived columns.
 */

import { describe, expect, test } from 'vitest';

import type { PivotField } from '@/types';

import { buildPivotOperations, fieldKey, type PivotQueryState } from './buildOperations';

function field(column: string, extra: Partial<PivotField> = {}): PivotField {
  return { column, datatype: 'String', id: column, ...extra };
}

function state(extra: Partial<PivotQueryState> = {}): PivotQueryState {
  return {
    columnFields: [],
    filters: [],
    filtersEnabled: false,
    limit: 0,
    limitEnabled: false,
    rowFields: [],
    sortBy: null,
    sortDescending: false,
    sortEnabled: false,
    valueFields: [field('revenue', { aggregation: 'sum', datatype: 'Float' })],
    ...extra,
  };
}

describe('buildPivotOperations', () => {
  test('rows only with a month bucket: withColumns, then groupBy on the period', () => {
    const ops = buildPivotOperations(
      state({ rowFields: [field('order_date', { granularity: 'month', role: 'time' })] }),
    );
    expect(ops).toEqual([
      {
        columns: [
          {
            expr: { column: 'order_date', fn: 'period', granularity: 'month' },
            name: 'order_date__month',
          },
        ],
        type: 'withColumns',
      },
      {
        aggs: [{ alias: 'sum', column: 'revenue', function: 'sum' }],
        by: ['order_date__month'],
        type: 'groupBy',
      },
    ]);
  });

  test('a bucketed column field pivots on the period column', () => {
    const ops = buildPivotOperations(
      state({
        columnFields: [field('created_at', { granularity: 'year', role: 'time' })],
        rowFields: [field('category')],
      }),
    );
    expect(ops[0]?.type).toBe('withColumns');
    expect(ops[1]).toEqual({
      agg: 'sum',
      columns: 'created_at__year',
      index: ['category'],
      type: 'pivot',
      values: 'revenue',
    });
  });

  test('fields without a granularity emit no withColumns; sort and limit follow', () => {
    const ops = buildPivotOperations(
      state({
        limit: 50,
        limitEnabled: true,
        rowFields: [field('category')],
        sortBy: 'sum',
        sortDescending: true,
        sortEnabled: true,
      }),
    );
    expect(ops.map((o) => o.type)).toEqual(['groupBy', 'sort', 'limit']);
    expect(fieldKey(field('category'))).toBe('category');
  });

  test('nothing to group on yields only the filters', () => {
    const ops = buildPivotOperations(
      state({
        filters: [{ column: 'region', id: 'f', op: 'eq', value: 'EU' }],
        filtersEnabled: true,
      }),
    );
    expect(ops.map((o) => o.type)).toEqual(['filter']);
  });
});
