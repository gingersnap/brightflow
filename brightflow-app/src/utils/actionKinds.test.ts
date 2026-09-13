/**
 * The two kind lists partition the semantic actions: nothing is in both,
 * and the reset is column-scoped so the store treats its event like the
 * other column edits.
 */

import { describe, expect, test } from 'vitest';

import { COLUMN_ACTION_KINDS, TABLE_ACTION_KINDS } from './actionKinds';

describe('action kind lists', () => {
  test('column and table kinds are disjoint', () => {
    const overlap = COLUMN_ACTION_KINDS.filter((k) => TABLE_ACTION_KINDS.includes(k));
    expect(overlap).toEqual([]);
  });

  test('the reset is a column action and settings a table action', () => {
    expect(COLUMN_ACTION_KINDS).toContain('reset_column_semantics');
    expect(TABLE_ACTION_KINDS).toContain('set_table_settings');
  });
});
