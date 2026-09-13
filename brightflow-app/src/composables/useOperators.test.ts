/**
 * Unit tests for the filter-operator catalogue in `useOperators.ts`.
 *
 * These pin the operator sets per logical type and the bucketing that
 * feeds them. They do not check that the backend accepts
 * these operator keys — that contract belongs to an API-level test, not here.
 */

import { describe, test, expect } from 'vitest';

import type { LogicalType } from '@/types/generated';

import { useOperators } from './useOperators';

const {
  getDefaultOperator,
  getOperator,
  getOperatorsForType,
  isFilterOp,
  operatorIsArray,
  operatorNeedsValue,
} = useOperators();

function keysFor(datatype: LogicalType | null | undefined): string[] {
  return getOperatorsForType(datatype).map((op) => op.value);
}

describe('getOperatorsForType', () => {
  test('string columns get the universal set plus contains and in', () => {
    expect(keysFor('String')).toEqual(['eq', 'ne', 'isNull', 'isNotNull', 'contains', 'in']);
  });

  test('int and float columns get the universal set plus comparisons and in', () => {
    const expected = ['eq', 'ne', 'isNull', 'isNotNull', 'gt', 'gte', 'lt', 'lte', 'in'];
    expect(keysFor('Integer')).toEqual(expected);
    expect(keysFor('Float')).toEqual(expected);
  });

  test('boolean columns get only the universal set — no contains, no in', () => {
    expect(keysFor('Boolean')).toEqual(['eq', 'ne', 'isNull', 'isNotNull']);
  });

  test('optional flags are materialized as booleans, not left undefined', () => {
    const isNull = getOperatorsForType('String').find((op) => op.value === 'isNull');
    expect(isNull).toEqual({ isArray: false, label: 'is null', noValue: true, value: 'isNull' });

    const eq = getOperatorsForType('String').find((op) => op.value === 'eq');
    expect(eq).toEqual({ isArray: false, label: 'equals', noValue: false, value: 'eq' });
  });

  test('Decimal takes the float operators', () => {
    expect(keysFor('Decimal')).toEqual(keysFor('Float'));
  });

  test('a missing type falls back to string', () => {
    /* A column whose type the backend omitted entirely. */
    const untyped: { datatype?: LogicalType } = {};

    expect(keysFor(null)).toEqual(keysFor('String'));
    expect(keysFor(untyped.datatype)).toEqual(keysFor('String'));
  });

  test('temporal types get comparison operators worded as before and after', () => {
    expect(keysFor('Date')).toEqual(['eq', 'ne', 'isNull', 'isNotNull', 'gt', 'gte', 'lt', 'lte']);
    expect(keysFor('DateTimeTz')).toEqual(keysFor('Date'));
    const labels = getOperatorsForType('Date').map((op) => op.label);
    expect(labels).toEqual([
      'on',
      'not on',
      'is null',
      'is not null',
      'after',
      'on or after',
      'before',
      'on or before',
    ]);
    // The same operator keeps its numeric wording elsewhere.
    expect(getOperatorsForType('Integer').find((op) => op.value === 'gt')?.label).toBe(
      'greater than',
    );
    expect(getDefaultOperator('Date')).toBe('gte');
  });
});

describe('getOperator', () => {
  test('returns the raw definition for a known key', () => {
    expect(getOperator('eq')).toEqual({
      label: 'equals',
      temporalLabel: 'on',
      types: ['string', 'int', 'float', 'boolean', 'temporal'],
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
    expect(getDefaultOperator('String')).toBe('contains');
    expect(getDefaultOperator(null)).toBe('contains');
  });

  test('numeric and boolean columns default to equals', () => {
    expect(getDefaultOperator('Integer')).toBe('eq');
    expect(getDefaultOperator('Float')).toBe('eq');
    expect(getDefaultOperator('Boolean')).toBe('eq');
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
