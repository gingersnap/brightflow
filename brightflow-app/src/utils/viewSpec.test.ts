/**
 * Unit tests for the saved-view spec: capture copies the stores, parse
 * accepts only this version's shape, apply writes back a copy, and the
 * one-line description reads the spec.
 */

import { describe, expect, test } from 'vitest';
import { reactive } from 'vue';

import {
  applyExploreView,
  captureExploreView,
  describeExploreView,
  parseExploreView,
  type PivotSnapshot,
  type QuerySnapshot,
} from './viewSpec';

function query(): QuerySnapshot {
  return {
    filters: [{ column: 'state', id: 'f1', op: 'eq', value: 'open' }],
    limit: 50,
    sections: {
      filter: { collapsed: false, enabled: true },
      limit: { collapsed: false, enabled: true },
      sort: { collapsed: true, enabled: true },
    },
    sortBy: 'created_at',
    sortDescending: true,
  };
}

function pivot(): PivotSnapshot {
  return {
    columnFields: [],
    decimalPlaces: 1,
    rowFields: [{ column: 'region', datatype: 'String', id: 'r1', role: 'dimension' }],
    showColumnTotals: true,
    showConditionalFormatting: false,
    showSubtotals: false,
    valueFields: [
      { aggregation: 'sum', column: 'revenue', datatype: 'Float', id: 'v1', role: 'measure' },
    ],
  };
}

describe('captureExploreView and applyExploreView', () => {
  test('round-trip without sharing objects with the stores', () => {
    const q = query();
    const p = pivot();
    const spec = captureExploreView(q, p);
    expect(spec.version).toBe(1);
    expect(spec.query.filters).toEqual(q.filters);
    expect(spec.query.filters).not.toBe(q.filters);

    const emptyQuery: QuerySnapshot = {
      filters: [],
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
    applyExploreView(spec, emptyQuery, emptyPivot);
    expect(emptyQuery.sortBy).toBe('created_at');
    expect(emptyQuery.filters).toEqual(q.filters);
    expect(emptyPivot.valueFields[0]?.aggregation).toBe('sum');
    expect(emptyPivot.decimalPlaces).toBe(1);
    // The stores got their own copy: editing them leaves the spec intact.
    emptyQuery.filters.pop();
    expect(spec.query.filters).toHaveLength(1);
  });
});

describe('captureExploreView from reactive state', () => {
  test('captures store state that is a Vue reactive proxy', () => {
    // The stores hand over reactive proxies; structuredClone would throw here.
    const liveQuery = reactive(query());
    const livePivot = reactive(pivot());
    const spec = captureExploreView(liveQuery, livePivot);
    expect(spec).toEqual(captureExploreView(query(), pivot()));
    // A plain object, not a proxy, and not shared with the source.
    expect(Object.getPrototypeOf(spec.pivot.rowFields[0])).toBe(Object.prototype);
    expect(spec.pivot.rowFields).not.toBe(livePivot.rowFields);
  });
});

describe('parseExploreView', () => {
  test('accepts this version and refuses anything else', () => {
    const spec = captureExploreView(query(), pivot());
    const wire: unknown = structuredClone(spec);
    expect(parseExploreView(wire)).toEqual(spec);
    expect(parseExploreView({ pivot: {}, query: {}, version: 2 })).toBeNull();
    expect(parseExploreView({ pivot: {}, query: { filters: 'no' }, version: 1 })).toBeNull();
    const damaged: unknown = {
      ...structuredClone(spec),
      pivot: { ...structuredClone(spec.pivot), decimalPlaces: 'two' },
    };
    expect(parseExploreView(damaged)).toBeNull();
    expect(parseExploreView(null)).toBeNull();
    expect(parseExploreView('{}')).toBeNull();
  });
});

describe('describeExploreView', () => {
  test('names filters, values, buckets and sort, or says plain table', () => {
    const full = captureExploreView(query(), pivot());
    expect(describeExploreView(full)).toBe('1 filter · 1 value · by region · sorted by created_at');
    const bareQuery = { ...query(), filters: [], sortBy: null };
    const barePivot = { ...pivot(), rowFields: [], valueFields: [] };
    const bare = captureExploreView(bareQuery, barePivot);
    expect(describeExploreView(bare)).toBe('plain table');
  });
});
