/** Tests for the committed-search term parser (chips syntax). */

import { describe, expect, test } from 'vitest';

import { parseTermInput } from './useTextExplore';

describe('parseTermInput', () => {
  test('bare words become include terms', () => {
    expect(parseTermInput('login crash')).toEqual([
      { exclude: false, text: 'login' },
      { exclude: false, text: 'crash' },
    ]);
  });

  test('quoted phrases stay whole, minus excludes, -"phrase" combines both', () => {
    expect(parseTermInput('"login page" -crash -"false alarm"')).toEqual([
      { exclude: false, text: 'login page' },
      { exclude: true, text: 'crash' },
      { exclude: true, text: 'false alarm' },
    ]);
  });

  test('sub-minimum-length terms are dropped as noise', () => {
    expect(parseTermInput('a login b')).toEqual([{ exclude: false, text: 'login' }]);
    expect(parseTermInput('- "x"')).toEqual([]);
  });
});
