/**
 * Unit tests for the filter-operator catalogue in `useOperators.ts`.
 *
 * These pin the operator sets per normalized column type and the dtype
 * normalization that feeds them. They do not check that the backend accepts
 * these operator keys — that contract belongs to an API-level test, not here.
 */

import { describe, test, expect } from 'vitest';

import { useOperators } from './useOperators';

const {
  getDefaultOperator,
  getOperator,
  getOperatorsForType,
  isFilterOp,
  operatorIsArray,
  operatorNeedsValue,
} = useOperators();

function keysFor(dtype: string | null | undefined): string[] {
  return getOperatorsForType(dtype).map((op) => op.value);
}

describe('getOperatorsForType', () => {
  test('string columns get the universal set plus contains and in', () => {
    expect(keysFor('string')).toEqual(['eq', 'ne', 'isNull', 'isNotNull', 'contains', 'in']);
  });

  test('int and float columns get the universal set plus comparisons and in', () => {
    const expected = ['eq', 'ne', 'isNull', 'isNotNull', 'gt', 'gte', 'lt', 'lte', 'in'];
    expect(keysFor('int')).toEqual(expected);
    expect(keysFor('float')).toEqual(expected);
  });

  test('boolean columns get only the universal set — no contains, no in', () => {
    expect(keysFor('boolean')).toEqual(['eq', 'ne', 'isNull', 'isNotNull']);
  });

  test('optional flags are materialized as booleans, not left undefined', () => {
    const isNull = getOperatorsForType('string').find((op) => op.value === 'isNull');
    expect(isNull).toEqual({ isArray: false, label: 'is null', noValue: true, value: 'isNull' });

    const eq = getOperatorsForType('string').find((op) => op.value === 'eq');
    expect(eq).toEqual({ isArray: false, label: 'equals', noValue: false, value: 'eq' });
  });

  test('string dtype aliases normalize to string', () => {
    expect(keysFor('varchar')).toEqual(keysFor('string'));
    expect(keysFor('utf8')).toEqual(keysFor('string'));
  });

  test('dtype matching is case-insensitive', () => {
    expect(keysFor('BIGINT')).toEqual(keysFor('int'));
  });

  test('missing or empty dtype falls back to string', () => {
    /* A column whose dtype the backend omitted entirely. */
    const untyped: { dtype?: string } = {};

    expect(keysFor(null)).toEqual(keysFor('string'));
    expect(keysFor(untyped.dtype)).toEqual(keysFor('string'));
    expect(keysFor('')).toEqual(keysFor('string'));
  });

  test('unrecognized dtype falls back to string', () => {
    /* 'date' is a real backend dtype with no normalization rule yet. */
    expect(keysFor('date')).toEqual(keysFor('string'));
  });
});

describe('getOperator', () => {
  test('returns the raw definition for a known key', () => {
    expect(getOperator('eq')).toEqual({
      label: 'equals',
      types: ['string', 'int', 'float', 'boolean'],
    });
  });

  test('omits noValue rather than setting it false on the raw definition', () => {
    /*
     * Unlike getOperatorsForType's normalized Operator, the raw def leaves
     * absent flags absent.
     */
    expect(getOperator('eq')?.noValue).toBeUndefined();
    expect(getOperator('isNull')?.noValue).toBe(true);
  });

  test('returns null for an unknown key', () => {
    expect(getOperator('nope')).toBeNull();
  });
});

describe('operatorNeedsValue', () => {
  test('null-check operators need no value', () => {
    expect(operatorNeedsValue('isNull')).toBe(false);
    expect(operatorNeedsValue('isNotNull')).toBe(false);
  });

  test('comparison operators need a value', () => {
    expect(operatorNeedsValue('eq')).toBe(true);
  });

  test('unknown operators default to needing a value', () => {
    expect(operatorNeedsValue('nope')).toBe(true);
  });
});

describe('operatorIsArray', () => {
  test('in takes an array', () => {
    expect(operatorIsArray('in')).toBe(true);
  });

  test('eq does not', () => {
    expect(operatorIsArray('eq')).toBe(false);
  });

  test('unknown operators default to non-array', () => {
    expect(operatorIsArray('nope')).toBe(false);
  });
});

describe('getDefaultOperator', () => {
  test('string columns default to contains', () => {
    expect(getDefaultOperator('string')).toBe('contains');
    expect(getDefaultOperator(null)).toBe('contains');
  });

  test('numeric and boolean columns default to equals', () => {
    expect(getDefaultOperator('int')).toBe('eq');
    expect(getDefaultOperator('float')).toBe('eq');
    expect(getDefaultOperator('boolean')).toBe('eq');
  });
});

describe('isFilterOp', () => {
  test('accepts known operators', () => {
    expect(isFilterOp('eq')).toBe(true);
    expect(isFilterOp('isNotNull')).toBe(true);
  });

  test('rejects unknown keys and the empty placeholder', () => {
    expect(isFilterOp('nope')).toBe(false);
    expect(isFilterOp('')).toBe(false);
  });
});
