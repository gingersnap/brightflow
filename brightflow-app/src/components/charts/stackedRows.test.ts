/**
 * Unit tests for by-position stacking of a nested pivot: bars follow the
 * given order, series count equals the deepest bar, short bars pad with
 * null, and every segment carries its name and share.
 */

import { describe, expect, test } from 'vitest';

import { DEFAULT_FIELD_SORT, orderRows } from '@/utils/pivotOrder';

import { stackedRowSeries } from './stackedRows';

// Category, subcategory, count
const rows: unknown[][] = [
  ['login', 'password', 5],
  ['billing', 'vat', 30],
  ['billing', 'other', 10],
  ['billing', 'refund', 1],
  ['billing', 'card', 9],
  ['other', 'other', 12],
  ['login', 'mfa', 15],
];

describe('stackedRowSeries', () => {
  const order = orderRows({
    rows,
    indexIdx: [0, 1],
    valueIdx: [2],
    sorts: [DEFAULT_FIELD_SORT, DEFAULT_FIELD_SORT],
  });
  const stacked = stackedRowSeries({ rows, outerIdx: 0, innerIdx: 1, valueIdx: 2, order });

  test('bars follow the order given, largest first', () => {
    expect(stacked.bars).toEqual(['billing', 'login', 'other']);
  });

  test('one series per position, padded with null where a bar is shorter', () => {
    expect(stacked.series).toHaveLength(4);
    expect(stacked.series.map((s) => s.data[2]?.name ?? null)).toEqual(['other', null, null, null]);
    expect(stacked.series[0]?.data.map((d) => d?.name)).toEqual(['vat', 'mfa', 'other']);
  });

  test('segments carry value and share of their bar', () => {
    const billing = stacked.segments[0] ?? [];
    expect(billing.map((s) => s.name)).toEqual(['vat', 'other', 'card', 'refund']);
    expect(billing[0]?.share).toBeCloseTo(0.6);
    expect(billing.reduce((sum, s) => sum + s.share, 0)).toBeCloseTo(1);
  });

  test('empty input gives no bars and no series', () => {
    const empty = stackedRowSeries({ rows: [], outerIdx: 0, innerIdx: 1, valueIdx: 2, order: [] });
    expect(empty).toEqual({ bars: [], segments: [], series: [] });
  });
});
