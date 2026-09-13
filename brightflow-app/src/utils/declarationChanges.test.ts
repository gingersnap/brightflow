/**
 * Unit tests for the declaration-change renderers shared by the table
 * picker and the Semantics page.
 */

import { describe, expect, test } from 'vitest';

import type { DeclarationDiff } from '@/types/generated';

import { changeDetail, changeLines, changeSummary } from './declarationChanges';

const diff: DeclarationDiff = {
  producer: 'connector:github',
  fromVersion: '0.2.0',
  toVersion: '0.3.0',
  changes: [
    { column: 'reactions_total', field: 'role', from: 'dimension', to: 'measure' },
    { column: 'reactions_total', field: 'label', to: 'Reactions' },
    { field: 'description', from: 'Issues', to: 'GitHub issues' },
  ],
};

describe('changeSummary', () => {
  test('names the versions and each changed column once', () => {
    expect(changeSummary(diff)).toBe('Declaration 0.2.0 → 0.3.0 changed reactions_total');
  });

  test('falls back to table settings and a lone version', () => {
    const tableOnly: DeclarationDiff = {
      producer: 'document:model',
      toVersion: '1.0.0',
      changes: [{ field: 'description', to: 'x' }],
    };
    expect(changeSummary(tableOnly)).toBe('Declaration 1.0.0 changed table settings');
  });
});

describe('changeLines', () => {
  test('renders one line per field with a dash for an absent side', () => {
    expect(changeLines(diff)).toEqual([
      'reactions_total.role: dimension → measure',
      'reactions_total.label: — → Reactions',
      'table.description: Issues → GitHub issues',
    ]);
    expect(changeDetail(diff).split('\n')).toHaveLength(3);
  });
});
