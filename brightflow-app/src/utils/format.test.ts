/**
 * Seed unit tests for the display-formatting helpers in `format.ts`.
 *
 * These pin the TypeScript output only; they do not cross-check against the
 * Rust `humanize_period` (engine `tree.rs`) — that would be an integration
 * test, out of scope for the co-located unit-test rule.
 */

import { describe, test, expect } from 'vitest';

import {
  formatDecimal,
  formatNumber,
  humanizePeriod,
  humanizeColumn,
  friendlyEngineError,
} from './format';

describe('formatNumber', () => {
  test('adds thousands separators for magnitudes >= 1000', () => {
    expect(formatNumber(1_234_567)).toBe('1,234,567');
  });

  test('preserves small-magnitude precision below 0.01', () => {
    expect(formatNumber(0.0042)).toBe('0.0042');
  });

  test('renders mid-range values with up to two decimals', () => {
    expect(formatNumber(12.34)).toBe('12.34');
  });

  test('passes NaN and Infinity through as their string form', () => {
    expect(formatNumber(Number.NaN)).toBe('NaN');
    expect(formatNumber(Number.POSITIVE_INFINITY)).toBe('Infinity');
    expect(formatNumber(Number.NEGATIVE_INFINITY)).toBe('-Infinity');
  });
});

describe('humanizePeriod', () => {
  test('week periods become "week N of YEAR"', () => {
    expect(humanizePeriod('2023-W12')).toBe('week 12 of 2023');
  });

  test('quarter periods become "QN YEAR"', () => {
    expect(humanizePeriod('2023-Q1')).toBe('Q1 2023');
  });

  test('month periods become "Month YEAR"', () => {
    expect(humanizePeriod('2023-03')).toBe('March 2023');
  });

  test('unrecognized periods pass through unchanged', () => {
    expect(humanizePeriod('custom')).toBe('custom');
  });
});

describe('humanizeColumn', () => {
  test('title-cases snake_case column names', () => {
    expect(humanizeColumn('order_total')).toBe('Order Total');
  });

  test('returns empty string for blank/underscore-only input', () => {
    expect(humanizeColumn('')).toBe('');
    expect(humanizeColumn('___')).toBe('');
  });

  test('trims leading/trailing underscores', () => {
    expect(humanizeColumn('_foo')).toBe('Foo');
    expect(humanizeColumn('foo_')).toBe('Foo');
  });
});

describe('friendlyEngineError', () => {
  test('schema-build prefix → schema message', () => {
    const { message, detail } = friendlyEngineError('Schema build failed: bad col');
    expect(message).toMatch(/column types/iu);
    expect(detail).toBe('Schema build failed: bad col');
  });

  test('table-not-found → sync prompt', () => {
    const { message, detail } = friendlyEngineError("Table 'foo' not found");
    expect(message).toMatch(/no stored data yet/iu);
    expect(detail).toBe("Table 'foo' not found");
  });

  test('no-data-files → exists-but-empty prompt', () => {
    const { message, detail } = friendlyEngineError('source has no data files');
    expect(message).toMatch(/no data yet/iu);
    expect(detail).toBe('source has no data files');
  });

  test('no-store-configured → server-config prompt', () => {
    const { message, detail } = friendlyEngineError('No data store configured');
    expect(message).toMatch(/without a data store/iu);
    expect(detail).toBe('No data store configured');
  });

  test('unknown error → generic fallback with detail preserved', () => {
    const raw = 'something totally unexpected went wrong';
    const { message, detail } = friendlyEngineError(raw);
    expect(message).toBe('The analysis failed unexpectedly.');
    expect(detail).toBe(raw);
  });
});

describe('formatDecimal', () => {
  test('caps decimals at the requested precision with en-US separators', () => {
    expect(formatDecimal(1234.5678, 2)).toBe('1,234.57');
    expect(formatDecimal(1234.5678, 0)).toBe('1,235');
    expect(formatDecimal(3, 2)).toBe('3');
  });

  test('non-finite values pass through as strings', () => {
    expect(formatDecimal(Number.NaN, 2)).toBe('NaN');
    expect(formatDecimal(Number.POSITIVE_INFINITY, 1)).toBe('Infinity');
  });
});
