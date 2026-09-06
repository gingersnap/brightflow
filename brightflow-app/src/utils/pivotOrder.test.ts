/**
 * Unit tests for pivot display order: nested row fields sort within their
 * parent, value vs label sorts in both directions, and the shared tie-break
 * (`other` back, null last) for rows and series alike.
 */

import { describe, expect, test } from 'vitest';

import { DEFAULT_FIELD_SORT, orderRows, orderSeries, sortKey } from './pivotOrder';

// Category, subcategory, count — the long group-by shape.
const rows: unknown[][] = [
  ['login', 'password', 5],
  ['billing', 'vat', 30],
  ['billing', 'other', 40],
  ['billing', 'refund', 1],
  ['other', 'other', 12],
  ['login', 'other', 2],
  ['login', 'mfa', 5],
  [null, null, 3],
];

function labels(order: number[], col: number): unknown[] {
  return order.map((i) => rows[i]?.[col]);
}

describe('orderRows', () => {
  test('groups by the first field and sorts groups by their total, largest first', () => {
    const order = orderRows({
      rows,
      indexIdx: [0, 1],
      valueIdx: [2],
      sorts: [DEFAULT_FIELD_SORT, DEFAULT_FIELD_SORT],
    });
    // Billing 71, login 12, other 12 (tie: other goes back), null 3 last.
    expect(labels(order, 0)).toEqual([
      'billing',
      'billing',
      'billing',
      'login',
      'login',
      'login',
      'other',
      null,
    ]);
  });

  test('sorts rows within each parent by the second field independently', () => {
    const order = orderRows({
      rows,
      indexIdx: [0, 1],
      valueIdx: [2],
      sorts: [DEFAULT_FIELD_SORT, DEFAULT_FIELD_SORT],
    });
    expect(labels(order, 1).slice(0, 3)).toEqual(['other', 'vat', 'refund']);
    // Login: password 5 and mfa 5 tie, text decides; other 2 last by value anyway.
    expect(labels(order, 1).slice(3, 6)).toEqual(['mfa', 'password', 'other']);
  });

  test('a value sort keeps other where its size puts it', () => {
    // Billing/other is the biggest subcategory under billing and stays first.
    const order = orderRows({
      rows,
      indexIdx: [0, 1],
      valueIdx: [2],
      sorts: [DEFAULT_FIELD_SORT, DEFAULT_FIELD_SORT],
    });
    expect(labels(order, 1)[0]).toBe('other');
  });

  test('smallest first reverses the totals but not the tie-break', () => {
    const order = orderRows({
      rows,
      indexIdx: [0, 1],
      valueIdx: [2],
      sorts: [{ by: 'value', descending: false }],
    });
    // Null 3, login 12, other 12 (other still after login), billing 71.
    expect([...new Set(labels(order, 0))]).toEqual([null, 'login', 'other', 'billing']);
  });

  test('label sorts go A→Z or Z→A with other and null always at the end', () => {
    const az = orderRows({
      rows,
      indexIdx: [0],
      valueIdx: [2],
      sorts: [{ by: 'label', descending: false }],
    });
    expect([...new Set(labels(az, 0))]).toEqual(['billing', 'login', 'other', null]);
    const za = orderRows({
      rows,
      indexIdx: [0],
      valueIdx: [2],
      sorts: [{ by: 'label', descending: true }],
    });
    expect([...new Set(labels(za, 0))]).toEqual(['login', 'billing', 'other', null]);
  });

  test('missing sorts fall back to largest first; empty input is empty', () => {
    const order = orderRows({ rows, indexIdx: [0], valueIdx: [2], sorts: [] });
    expect(labels(order, 0)[0]).toBe('billing');
    expect(orderRows({ rows: [], indexIdx: [0], valueIdx: [1], sorts: [] })).toEqual([]);
  });

  test('non-numeric value cells count as zero rather than NaN', () => {
    const dirty: unknown[][] = [
      ['a', 'x'],
      ['b', 7],
    ];
    const order = orderRows({
      rows: dirty,
      indexIdx: [0],
      valueIdx: [1],
      sorts: [DEFAULT_FIELD_SORT],
    });
    expect(order.map((i) => dirty[i]?.[0])).toEqual(['b', 'a']);
  });
});

describe('orderSeries', () => {
  // Category × subcategory wide: index then three series columns.
  const wide: unknown[][] = [
    ['billing', 30, null, 40],
    ['login', null, 5, 2],
  ];
  const names = ['category', 'vat', 'password', 'other'];

  test('by total, largest first, other back among equals', () => {
    expect(
      orderSeries({ rows: wide, seriesIdx: [1, 2, 3], names, sort: DEFAULT_FIELD_SORT }),
    ).toEqual([3, 1, 2]);
  });

  test('by name keeps other at the end', () => {
    expect(
      orderSeries({
        rows: wide,
        seriesIdx: [1, 2, 3],
        names,
        sort: { by: 'label', descending: false },
      }),
    ).toEqual([2, 1, 3]);
  });
});

test('sortKey is stable and distinct per combination', () => {
  expect(sortKey(DEFAULT_FIELD_SORT)).toBe('value:desc');
  expect(sortKey({ by: 'label', descending: false })).toBe('label:asc');
});
