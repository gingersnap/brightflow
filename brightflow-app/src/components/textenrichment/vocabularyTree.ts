/**
 * Pure helpers behind VocabularyTree: merge one vocabulary level's stored
 * entries with the row counts the classifier produced and the health flags
 * computed over them, and pick the parents a subcategory fan-out may start
 * on. Kept out of the component so the merge rules are unit-testable.
 *
 * The flags are only meaningful once the counts were computed against the
 * current vocabulary: right after a proposal every new entry holds zero rows
 * until the next run, and that is "not run yet", not "unused". Callers pass
 * `stale` for that and every badge is suppressed while it is set.
 */

import type {
  TaxonomyCategory,
  ValueCount,
  VocabularyLevelHealth,
  VocabularyParentHealth,
} from '@/types/generated';

export type EntryBadge =
  | { kind: 'too-small'; text: string }
  | { kind: 'unused'; text: string }
  | { kind: 'out-of-band'; text: string };

/**
 * The reserved `other` category as a tree entry. Never a stored row: id 0 is
 * the root sentinel, which is also where its subcategories hang (kind
 * `subcategory`, parent 0), so `children(OTHER_ENTRY)` works unchanged. It
 * cannot be renamed, redefined, frozen or deleted; `frozen` keeps the
 * generic guards on those buttons honest.
 */
export const OTHER_ENTRY: TaxonomyCategory = {
  id: 0,
  kind: 'category',
  parentId: 0,
  name: 'other',
  frozen: true,
};

export function isOtherEntry(entry: TaxonomyCategory): boolean {
  return entry.id === OTHER_ENTRY.id;
}

export interface EntryRow {
  entry: TaxonomyCategory;
  /** Rows classified into this entry; null when the level has no row grain. */
  rows: number | null;
  /** Share of the level's rows (`other` included); null with `rows`. */
  share: number | null;
  badge: EntryBadge | null;
}

export interface LevelInput {
  /** Stored entries at this level, in display order. */
  entries: TaxonomyCategory[];
  /**
   * The level's value_counts, `other` included (which is why it is the share
   * denominator); null means the level has no row grain (products,
   * competitors, feedback categories).
   */
  counts: ValueCount[] | null;
  health: VocabularyLevelHealth | null;
  /** True while the classify function has rows to recompute — flags off. */
  stale: boolean;
}

/** One row per stored entry, in the order given. */
export function buildLevelRows({ entries, counts, health, stale }: LevelInput): EntryRow[] {
  const byValue = new Map<string, number>();
  let total = 0;
  for (const c of counts ?? []) {
    byValue.set(c.value, c.rows);
    total += c.rows;
  }
  const flags: Flags = { tooSmall: new Map(), outOfBand: new Map() };
  if (health != null) {
    for (const t of health.tooSmall) {
      flags.tooSmall.set(t.name, t.rows);
    }
    for (const u of health.unbalanced) {
      flags.outOfBand.set(u.name, u.share);
    }
  }
  return entries.map((entry) => {
    const rows = counts == null ? null : (byValue.get(entry.name) ?? 0);
    const share = rows == null || total === 0 ? null : rows / total;
    const badge = stale || rows == null ? null : badgeFor(entry.name, rows, flags);
    return { entry, rows, share, badge };
  });
}

/** Health findings by entry name: rows for too-small, share for the band. */
interface Flags {
  tooSmall: Map<string, number>;
  outOfBand: Map<string, number>;
}

/** Too-small wins over the band; a zero-row band miss is "unused". */
function badgeFor(name: string, rows: number, flags: Flags): EntryBadge | null {
  const small = flags.tooSmall.get(name);
  if (small != null) {
    return { kind: 'too-small', text: `${small} rows — too small to be a group` };
  }
  const band = flags.outOfBand.get(name);
  if (band == null) {
    return null;
  }
  if (rows === 0) {
    return { kind: 'unused', text: 'no rows — unused' };
  }
  return { kind: 'out-of-band', text: `${Math.round(band * 100)}% — share out of band` };
}

/**
 * Categories a subcategory proposal may run on: those with at least
 * `minRows` rows classified under them. Having subcategories already does
 * not disqualify a parent — define is an upsert by name and the prompt says
 * not to re-propose existing entries, so a re-run only costs tokens.
 */
export function eligibleParents(
  categories: TaxonomyCategory[],
  perParent: VocabularyParentHealth[],
  minRows: number,
): TaxonomyCategory[] {
  const rowsUnder = new Map<string, number>();
  for (const p of perParent) {
    rowsUnder.set(p.parent, p.health.rows);
  }
  return categories.filter((c) => (rowsUnder.get(c.name) ?? 0) >= minRows);
}

/** Largest first; entries without counts sort as zero; ties by name. */
export function sortBySize(rows: EntryRow[]): EntryRow[] {
  return [...rows].toSorted(
    (a, b) => (b.rows ?? 0) - (a.rows ?? 0) || a.entry.name.localeCompare(b.entry.name),
  );
}
