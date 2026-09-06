/**
 * Unit tests for the stacked-series colour rules: when series are confined
 * to one bar, how they rank inside it, how opacity steps, and how surplus
 * series fold into one "Other".
 */

import { describe, expect, test } from 'vitest';

import {
  foldSeries,
  foldedColumn,
  isOtherName,
  opacityForRank,
  rankWithinBars,
  seriesConfinedToOneBar,
} from './chartColor';

// Category rows, subcategory columns — the wide parent/child pivot.
const names = ['category', 'vat', 'refund', 'password', 'other', 'unused'];
const wide: unknown[][] = [
  ['billing', 30, 5, null, 4, null],
  ['login', null, null, 8, 2, null],
];
const series = [1, 2, 3, 4, 5];

describe('seriesConfinedToOneBar', () => {
  test('a parent/child pivot is confined; other and empty series are exempt', () => {
    const home = seriesConfinedToOneBar(wide, series, names);
    expect(home).not.toBeNull();
    expect(home?.get(1)).toBe(0);
    expect(home?.get(3)).toBe(1);
    expect(home?.get(4)).toBe(-1);
    expect(home?.get(5)).toBe(-1);
  });

  test('a series with values in two bars means not confined', () => {
    const stateByCategory: unknown[][] = [
      ['open', 10, 3],
      ['closed', 7, 9],
    ];
    expect(seriesConfinedToOneBar(stateByCategory, [1, 2], ['state', 'a', 'b'])).toBeNull();
  });
});

describe('rankWithinBars and opacityForRank', () => {
  test('ranks by value inside each bar, largest first', () => {
    const home = seriesConfinedToOneBar(wide, series, names);
    if (home == null) {
      throw new Error('expected confined');
    }
    const ranks = rankWithinBars(wide, series, home);
    expect(ranks.get(1)).toEqual({ rank: 0, count: 2 });
    expect(ranks.get(2)).toEqual({ rank: 1, count: 2 });
    expect(ranks.get(3)).toEqual({ rank: 0, count: 1 });
    expect(ranks.has(4)).toBe(false);
  });

  test('opacity steps from 1 to the floor and a single segment is opaque', () => {
    expect(opacityForRank(0, 1)).toBe(1);
    expect(opacityForRank(0, 4)).toBe(1);
    expect(opacityForRank(3, 4)).toBeCloseTo(0.35);
    expect(opacityForRank(1, 3)).toBeGreaterThan(opacityForRank(2, 3));
  });
});

describe('foldSeries', () => {
  test('keeps the first N non-other series and folds the rest', () => {
    const f = foldSeries([1, 2, 3, 4, 5], names, 2);
    expect(f.kept).toEqual([1, 2]);
    expect(f.folded).toEqual([3, 4, 5]);
    expect(foldedColumn(wide, f.folded)).toEqual([4, 10]);
  });

  test('a lone other is kept as a real series, and nothing folds under the cap', () => {
    expect(foldSeries([1, 4], names, 5)).toEqual({ kept: [1, 4], folded: [] });
    expect(foldSeries([1, 2, 3], names, 12)).toEqual({ kept: [1, 2, 3], folded: [] });
  });
});

test('isOtherName matches other in any case and null', () => {
  expect(isOtherName('Other')).toBe(true);
  expect(isOtherName(null)).toBe(true);
  expect(isOtherName('others')).toBe(false);
});
