/** Tests for the {{col:Name}} prompt-reference helpers. */

import { describe, expect, test } from 'vitest';

import { extractColumnRefs, insertColumnRef } from './promptTokens';

describe('extractColumnRefs', () => {
  test('refs come back in order, deduplicated, trimmed', () => {
    /*
     * Parity fixture with the Rust side: the same input/expectation lives in
     * `extracts_refs_in_order_deduplicated` (engine enrichment/function.rs),
     * so a divergence in either parser fails a named test on both sides.
     */
    expect(extractColumnRefs('{{col:Title}} and {{col: Body }} then {{col:Title}}')).toEqual([
      'Title',
      'Body',
    ]);
    expect(extractColumnRefs('no refs {{col:unterminated')).toEqual([]);
    expect(extractColumnRefs('')).toEqual([]);
  });
});

describe('insertColumnRef', () => {
  test('inserts at the cursor and advances it past the ref', () => {
    const { text, cursor } = insertColumnRef('before after', 7, 'Title');
    expect(text).toBe('before {{col:Title}}after');
    expect(cursor).toBe(7 + '{{col:Title}}'.length);
  });

  test('clamps out-of-range cursors to the template bounds', () => {
    expect(insertColumnRef('ab', 99, 'X').text).toBe('ab{{col:X}}');
    expect(insertColumnRef('ab', -5, 'X').text).toBe('{{col:X}}ab');
  });
});
