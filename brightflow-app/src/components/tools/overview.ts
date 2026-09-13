/**
 * Pure helpers for a source's Overview page: each table from the source
 * list joined with its signals from the overview endpoint, the coverage
 * line a card shows, and a relative time for "last changed". Nothing here
 * fetches; the page renders what these return.
 */

import type { SourceTable } from '@/types';
import type { SemanticCoverage, TableSignals } from '@/types/generated';

/** A table as the Overview shows it: the listing plus its signals, if any. */
export interface OverviewTable {
  table: SourceTable;
  signals: TableSignals | null;
}

/** Join by name; a table without signals (endpoint not loaded yet) still renders. */
export function mergeSignals(tables: SourceTable[], signals: TableSignals[]): OverviewTable[] {
  const byName = new Map(signals.map((s) => [s.name, s]));
  return tables.map((table) => ({ signals: byName.get(table.name) ?? null, table }));
}

/** "8 of 12 columns described" — or, with everything covered, "all 12 columns described". */
export function coverageText(coverage: SemanticCoverage): string {
  if (coverage.columns === 0) {
    return 'no columns';
  }
  const noun = coverage.columns === 1 ? 'column' : 'columns';
  if (coverage.described === coverage.columns) {
    return `all ${coverage.columns} ${noun} described`;
  }
  return `${coverage.described} of ${coverage.columns} ${noun} described`;
}

/** Share of columns described, 0 to 1, for the coverage bar. */
export function coverageShare(coverage: SemanticCoverage): number {
  return coverage.columns === 0 ? 0 : coverage.described / coverage.columns;
}

/**
 * The catalog writes `datetime('now')`: "YYYY-MM-DD HH:MM:SS" in UTC with
 * no zone marker. Read it as UTC; anything else parses as the platform does.
 */
export function parseCatalogTime(text: string): Date | null {
  const iso = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/u.test(text)
    ? `${text.replace(' ', 'T')}Z`
    : text;
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? null : date;
}

/** "just now", "5 min ago", "3 h ago", "2 days ago", else the date. */
export function relativeTime(then: Date, now: Date = new Date()): string {
  const seconds = Math.round((now.getTime() - then.getTime()) / 1000);
  if (seconds < 60) {
    return 'just now';
  }
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) {
    return `${minutes} min ago`;
  }
  const hours = Math.round(minutes / 60);
  if (hours < 24) {
    return `${hours} h ago`;
  }
  const days = Math.round(hours / 24);
  if (days < 14) {
    return `${days} day${days === 1 ? '' : 's'} ago`;
  }
  return then.toLocaleDateString();
}
