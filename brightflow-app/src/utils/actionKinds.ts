/**
 * The semantic action kinds, grouped by what they act on. One list each, so
 * the command palette (which offers them) and the dataset store (which
 * patches a loaded table from their events) cannot keep drifting copies.
 * The kinds themselves are the backend's `ACTION_KINDS`; these lists only
 * say which of them belong to a column and which to the table.
 */

/** Kinds that name a column: `params.column` is set on their events. */
export const COLUMN_ACTION_KINDS: readonly string[] = [
  'set_column_role',
  'set_column_label',
  'set_column_description',
  'set_kpi',
  'set_column_polarity',
  'reset_column_semantics',
];

/** Kinds that act on the table as a whole. */
export const TABLE_ACTION_KINDS: readonly string[] = ['set_table_settings'];
