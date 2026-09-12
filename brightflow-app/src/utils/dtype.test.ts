/**
 * Tests for the shared dtype normalization — including the i64/f64 aliases
 * whose omission from hand-copied lists caused two live bugs.
 */

import { describe, expect, test } from 'vitest';

import {
  isFloatDtype,
  isNumericDtype,
  isStringDtype,
  isTemporalDtype,
  normalizeDtype,
} from './dtype';

describe('normalizeDtype', () => {
  test('maps aliases to their buckets', () => {
    expect(normalizeDtype('i64')).toBe('int');
    expect(normalizeDtype('bigint')).toBe('int');
    expect(normalizeDtype('number')).toBe('int');
    expect(normalizeDtype('f64')).toBe('float');
    expect(normalizeDtype('decimal')).toBe('float');
    expect(normalizeDtype('utf8')).toBe('string');
    expect(normalizeDtype('Boolean')).toBe('boolean');
  });

  test('missing or unknown dtypes fall back to string', () => {
    expect(normalizeDtype(null)).toBe('string');
    expect(normalizeDtype('')).toBe('string');
    expect(normalizeDtype('datetime')).toBe('string');
  });
});

describe('isNumericDtype / isFloatDtype', () => {
  test('i64 and f64 are numeric — the regression the shared list fixes', () => {
    expect(isNumericDtype('i64')).toBe(true);
    expect(isNumericDtype('f64')).toBe(true);
    expect(isNumericDtype('utf8')).toBe(false);
  });

  test('float detection separates int from float', () => {
    expect(isFloatDtype('f64')).toBe(true);
    expect(isFloatDtype('float')).toBe(true);
    expect(isFloatDtype('i64')).toBe(false);
  });
});

describe('isStringDtype', () => {
  test('matches explicit string aliases only', () => {
    expect(isStringDtype('utf8')).toBe(true);
    expect(isStringDtype('varchar')).toBe(true);
    expect(isStringDtype('str')).toBe(true);
    // Unknown dtypes are NOT strings here (unlike normalizeDtype's fallback):
    // A datetime column must not be offered as a text column.
    expect(isStringDtype('datetime')).toBe(false);
    expect(isStringDtype(null)).toBe(false);
  });
});

describe('isTemporalDtype', () => {
  test('matches the typed date dtypes only', () => {
    expect(isTemporalDtype('date')).toBe(true);
    expect(isTemporalDtype('Datetime')).toBe(true);
    expect(isTemporalDtype('string')).toBe(false);
    expect(isTemporalDtype('duration')).toBe(false);
    expect(isTemporalDtype(null)).toBe(false);
  });
});
