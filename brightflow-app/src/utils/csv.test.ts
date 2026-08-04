/** Tests for CSV assembly: quoting, embedded delimiters, and non-strings. */

import { describe, expect, test } from 'vitest';

import { buildCsv, escapeCsvCell } from './csv';

describe('escapeCsvCell', () => {
  test('passes plain values through and blanks nullish', () => {
    expect(escapeCsvCell('plain')).toBe('plain');
    expect(escapeCsvCell(42)).toBe('42');
    expect(escapeCsvCell(null)).toBe('');
  });

  test('quotes delimiters, quotes, and newlines', () => {
    expect(escapeCsvCell('a,b')).toBe('"a,b"');
    expect(escapeCsvCell('say "hi"')).toBe('"say ""hi"""');
    expect(escapeCsvCell('line1\nline2')).toBe('"line1\nline2"');
  });

  test('serializes non-string values via JSON', () => {
    expect(escapeCsvCell(true)).toBe('true');
    expect(escapeCsvCell({ a: 1 })).toBe('"{""a"":1}"');
  });
});

describe('buildCsv', () => {
  test('joins a header row and data rows', () => {
    const csv = buildCsv(
      ['name', 'amount'],
      [
        ['alpha', 1],
        ['b,eta', 2],
      ],
    );
    expect(csv).toBe('name,amount\nalpha,1\n"b,eta",2');
  });

  test('empty rows yield just the header', () => {
    expect(buildCsv(['a'], [])).toBe('a');
  });
});
