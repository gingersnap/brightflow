/**
 * Unit tests for the Overview page's helpers: the signals join, the
 * coverage line and the relative time.
 */

import { describe, expect, test } from 'vitest';

import type { SourceTable } from '@/types';
import type { TableSignals } from '@/types/generated';

import {
  coverageShare,
  coverageText,
  mergeSignals,
  parseCatalogTime,
  relativeTime,
} from './overview';

const at = (iso: string): Date => new Date(iso);

function table(name: string): SourceTable {
  return { enrichable: false, name, numRows: 3 };
}

function signals(name: string): TableSignals {
  return {
    coverage: { columns: 4, described: 1, withRole: 4 },
    name,
    pendingProposals: 2,
    updatedAt: '2026-09-13 10:00:00',
  };
}

describe('mergeSignals', () => {
  test('joins by name and keeps tables the endpoint did not cover', () => {
    const merged = mergeSignals([table('issues'), table('orders')], [signals('orders')]);
    expect(merged.map((m) => [m.table.name, m.signals?.pendingProposals ?? null])).toEqual([
      ['issues', null],
      ['orders', 2],
    ]);
  });
});

describe('coverageText and coverageShare', () => {
  test('phrase the described count against the schema', () => {
    expect(coverageText({ columns: 12, described: 8, withRole: 12 })).toBe(
      '8 of 12 columns described',
    );
    expect(coverageText({ columns: 12, described: 12, withRole: 12 })).toBe(
      'all 12 columns described',
    );
    expect(coverageText({ columns: 1, described: 0, withRole: 1 })).toBe('0 of 1 column described');
    expect(coverageText({ columns: 0, described: 0, withRole: 0 })).toBe('no columns');
    expect(coverageShare({ columns: 4, described: 1, withRole: 4 })).toBe(0.25);
    expect(coverageShare({ columns: 0, described: 0, withRole: 0 })).toBe(0);
  });
});

describe('parseCatalogTime and relativeTime', () => {
  test('read the catalog timestamp as UTC and phrase the distance', () => {
    const then = parseCatalogTime('2026-09-13 10:00:00');
    expect(then?.toISOString()).toBe('2026-09-13T10:00:00.000Z');
    expect(parseCatalogTime('garbage')).toBeNull();
    expect(relativeTime(at('2026-09-13T10:00:00Z'), at('2026-09-13T10:00:30Z'))).toBe('just now');
    expect(relativeTime(at('2026-09-13T10:00:00Z'), at('2026-09-13T10:05:00Z'))).toBe('5 min ago');
    expect(relativeTime(at('2026-09-13T10:00:00Z'), at('2026-09-13T13:00:00Z'))).toBe('3 h ago');
    expect(relativeTime(at('2026-09-13T10:00:00Z'), at('2026-09-14T10:00:00Z'))).toBe('1 day ago');
    expect(relativeTime(at('2026-09-01T10:00:00Z'), at('2026-09-13T10:00:00Z'))).toBe(
      '12 days ago',
    );
  });
});
