/**
 * Unit tests for the vocabulary-tree merge rules: counts and shares per
 * entry, which badge wins, that staleness silences every badge, and which
 * parents a subcategory fan-out picks.
 */

import { describe, expect, test } from 'vitest';

import type {
  TaxonomyCategory,
  VocabularyLevelHealth,
  VocabularyParentHealth,
} from '@/types/generated';

import { buildLevelRows, eligibleParents, OTHER_ENTRY, sortBySize } from './vocabularyTree';

function entry(id: number, name: string, kind = 'category'): TaxonomyCategory {
  return { id, kind, parentId: 0, name, frozen: false };
}

function level(partial: Partial<VocabularyLevelHealth> = {}): VocabularyLevelHealth {
  return {
    kind: 'category',
    entries: 0,
    cap: 10,
    rows: 0,
    otherRate: 0,
    maxShare: 0,
    minShare: 0,
    unbalanced: [],
    tooSmall: [],
    ...partial,
  };
}

describe('buildLevelRows', () => {
  const entries = [entry(1, 'billing'), entry(2, 'login'), entry(3, 'tiny'), entry(4, 'unused')];
  const counts = [
    { value: 'billing', rows: 50 },
    { value: 'login', rows: 30 },
    { value: 'tiny', rows: 3 },
    { value: 'other', rows: 17 },
  ];
  const health = level({
    rows: 100,
    unbalanced: [
      { name: 'billing', share: 0.5 },
      { name: 'unused', share: 0 },
    ],
    tooSmall: [{ name: 'tiny', rows: 3 }],
  });

  test('counts and shares use the level total, other included', () => {
    const rows = buildLevelRows({ entries, counts, health, stale: false });
    expect(rows.map((r) => r.rows)).toEqual([50, 30, 3, 0]);
    expect(rows[0]?.share).toBeCloseTo(0.5);
    expect(rows[3]?.share).toBe(0);
  });

  test('too-small wins over the band; unused is a zero-row band miss', () => {
    const rows = buildLevelRows({ entries, counts, health, stale: false });
    expect(rows.map((r) => r.badge?.kind ?? null)).toEqual([
      'out-of-band',
      null,
      'too-small',
      'unused',
    ]);
    expect(rows[2]?.badge?.text).toContain('3 rows');
  });

  test('stale silences every badge but keeps the counts', () => {
    const rows = buildLevelRows({ entries, counts, health, stale: true });
    expect(rows.every((r) => r.badge == null)).toBe(true);
    expect(rows[0]?.rows).toBe(50);
  });

  test('a level without row grain has no counts and no badges', () => {
    const rows = buildLevelRows({
      entries: [entry(9, 'acme', 'competitor')],
      counts: null,
      health: null,
      stale: false,
    });
    expect(rows).toEqual([
      { entry: entry(9, 'acme', 'competitor'), rows: null, share: null, badge: null },
    ]);
  });

  test('no counts yet means zero rows and a null share, never NaN', () => {
    const rows = buildLevelRows({
      entries: entries.slice(0, 1),
      counts: [],
      health: null,
      stale: false,
    });
    expect(rows[0]?.rows).toBe(0);
    expect(rows[0]?.share).toBeNull();
  });
});

describe('eligibleParents', () => {
  const cats = [entry(1, 'billing'), entry(2, 'login'), entry(3, 'new')];
  const perParent: VocabularyParentHealth[] = [
    { parent: 'billing', health: level({ kind: 'subcategory', rows: 120 }) },
    { parent: 'login', health: level({ kind: 'subcategory', rows: 50 }) },
  ];

  test('keeps parents at or above the minimum, drops the rest and the unknown', () => {
    expect(eligibleParents(cats, perParent, 50).map((c) => c.name)).toEqual(['billing', 'login']);
    expect(eligibleParents(cats, perParent, 51).map((c) => c.name)).toEqual(['billing']);
  });

  test('only rows under the parent decide — never how many children it has', () => {
    // A parent with children looks exactly like one without to this function;
    // The caller passes root categories and the decision reads per-parent rows.
    const perParentWithChildren: VocabularyParentHealth[] = [
      { parent: 'billing', health: level({ kind: 'subcategory', rows: 120, entries: 6 }) },
    ];
    expect(eligibleParents(cats, perParentWithChildren, 50).map((c) => c.name)).toEqual([
      'billing',
    ]);
  });
});

describe('sortBySize', () => {
  test('largest first, ties and missing counts by name', () => {
    const rows = buildLevelRows({
      entries: [entry(1, 'zeta'), entry(2, 'alpha'), entry(3, 'mid'), OTHER_ENTRY],
      counts: [
        { value: 'alpha', rows: 5 },
        { value: 'zeta', rows: 5 },
        { value: 'other', rows: 40 },
      ],
      health: null,
      stale: false,
    });
    expect(sortBySize(rows).map((r) => r.entry.name)).toEqual(['other', 'alpha', 'zeta', 'mid']);
    // No row grain at all: pure name order.
    const bare = buildLevelRows({
      entries: [entry(1, 'b', 'competitor'), entry(2, 'a', 'competitor')],
      counts: null,
      health: null,
      stale: false,
    });
    expect(sortBySize(bare).map((r) => r.entry.name)).toEqual(['a', 'b']);
  });

  test('other never carries a badge, even when its share is out of band', () => {
    const rows = buildLevelRows({
      entries: [OTHER_ENTRY],
      counts: [{ value: 'other', rows: 90 }],
      health: level({ rows: 100, otherRate: 0.9 }),
      stale: false,
    });
    expect(rows[0]?.badge).toBeNull();
    expect(rows[0]?.share).toBeCloseTo(1);
  });
});

describe('eligibleParents with other', () => {
  test('other is a parent like any other when enough rows landed there', () => {
    const perParent: VocabularyParentHealth[] = [
      { parent: 'other', health: level({ kind: 'subcategory', rows: 60 }) },
    ];
    expect(eligibleParents([entry(1, 'billing'), OTHER_ENTRY], perParent, 50)).toEqual([
      OTHER_ENTRY,
    ]);
  });
});
