/**
 * One-line and per-field renderings of a producer's re-declaration
 * (`DeclarationDiff`), shared by the table picker's card and the Semantics
 * page. Pure functions over the generated type.
 */

import type { DeclarationDiff } from '@/types/generated';

/** "Declaration 0.2.0 → 0.3.0 changed reactions_total, created_at". */
export function changeSummary(diff: DeclarationDiff): string {
  const columns: string[] = [];
  for (const change of diff.changes) {
    if (change.column != null && !columns.includes(change.column)) {
      columns.push(change.column);
    }
  }
  const versions =
    diff.fromVersion != null && diff.toVersion != null
      ? `${diff.fromVersion} → ${diff.toVersion}`
      : (diff.toVersion ?? '');
  const what = columns.length > 0 ? columns.join(', ') : 'table settings';
  return `Declaration ${versions} changed ${what}`.replace('  ', ' ');
}

/** One line per changed field, for a tooltip or a detail list. */
export function changeLines(diff: DeclarationDiff): string[] {
  return diff.changes.map(
    (c) => `${c.column ?? 'table'}.${c.field}: ${c.from ?? '—'} → ${c.to ?? '—'}`,
  );
}

/** The lines joined, for a `title` attribute. */
export function changeDetail(diff: DeclarationDiff): string {
  return changeLines(diff).join('\n');
}
