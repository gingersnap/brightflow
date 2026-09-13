/**
 * Tests for the logical-type predicates: the four operator buckets, the
 * numeric and float distinctions formatting relies on, and that temporal
 * types are neither strings nor numbers.
 */

import { describe, expect, test } from 'vitest';

import { isFloatType, isNumericType, isStringType, isTemporalType, normalizeType } from './dtype';

describe('normalizeType', () => {
  test('folds the ten logical types into the operator buckets', () => {
    expect(normalizeType('Integer')).toBe('int');
    expect(normalizeType('Float')).toBe('float');
    expect(normalizeType('Decimal')).toBe('float');
    expect(normalizeType('Boolean')).toBe('boolean');
    expect(normalizeType('String')).toBe('string');
  });

  test('temporal types have their own bucket; opaque and missing fall back to string', () => {
    expect(normalizeType('DateTime')).toBe('temporal');
    expect(normalizeType('Date')).toBe('temporal');
    expect(normalizeType('Opaque')).toBe('string');
    expect(normalizeType(null)).toBe('string');
  });
});

describe('isNumericType / isFloatType', () => {
  test('integers and floats are numeric', () => {
    expect(isNumericType('Integer')).toBe(true);
    expect(isNumericType('Float')).toBe(true);
    expect(isNumericType('Decimal')).toBe(true);
    expect(isNumericType('String')).toBe(false);
  });

  test('float detection separates int from float', () => {
    expect(isFloatType('Float')).toBe(true);
    expect(isFloatType('Decimal')).toBe(true);
    expect(isFloatType('Integer')).toBe(false);
    expect(isFloatType(null)).toBe(false);
  });
});

describe('isStringType', () => {
  test('matches String only', () => {
    expect(isStringType('String')).toBe(true);
    expect(isStringType('DateTime')).toBe(false);
    expect(isStringType('Opaque')).toBe(false);
    expect(isStringType(null)).toBe(false);
  });
});

describe('isTemporalType', () => {
  test('matches the four temporal types only', () => {
    expect(isTemporalType('Date')).toBe(true);
    expect(isTemporalType('Time')).toBe(true);
    expect(isTemporalType('DateTime')).toBe(true);
    expect(isTemporalType('DateTimeTz')).toBe(true);
    expect(isTemporalType('String')).toBe(false);
    expect(isTemporalType(null)).toBe(false);
  });
});
