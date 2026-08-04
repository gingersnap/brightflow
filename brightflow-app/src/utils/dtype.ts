/**
 * Backend dtype normalization, shared by every component that branches on
 * column type.
 *
 * One alias list (Polars i64/f64/utf8, SQL-ish names, and the loose 'number')
 * so call sites cannot drift into partial copies — two live bugs came from
 * hand-copied lists that missed i64/f64. `isStringDtype` matches explicit
 * string aliases only, while `normalizeDtype` falls back to 'string' for
 * unknown dtypes (the operator picker's historical behavior); date-like
 * columns therefore get string operators without being offered as text
 * columns.
 */

export type NormalizedDtype = 'int' | 'float' | 'string' | 'boolean';

const INT_ALIASES = new Set(['int', 'integer', 'bigint', 'i64', 'i32', 'number']);
const FLOAT_ALIASES = new Set(['float', 'double', 'decimal', 'f64', 'f32']);
const STRING_ALIASES = new Set(['string', 'str', 'text', 'varchar', 'utf8']);
const BOOL_ALIASES = new Set(['bool', 'boolean']);

/** Normalize a backend dtype; unknown or missing dtypes fall back to 'string'. */
export function normalizeDtype(dtype: string | null | undefined): NormalizedDtype {
  if (dtype == null || dtype === '') {
    return 'string';
  }
  const t = dtype.toLowerCase();
  if (INT_ALIASES.has(t)) {
    return 'int';
  }
  if (FLOAT_ALIASES.has(t)) {
    return 'float';
  }
  if (BOOL_ALIASES.has(t)) {
    return 'boolean';
  }
  return 'string';
}

/** Int or float (includes the loose 'number' alias). */
export function isNumericDtype(dtype: string | null | undefined): boolean {
  const t = normalizeDtype(dtype);
  return t === 'int' || t === 'float';
}

/** Float only — used to pick decimal formatting. */
export function isFloatDtype(dtype: string | null | undefined): boolean {
  return normalizeDtype(dtype) === 'float';
}

/**
 * Explicitly string-typed only (no unknown-dtype fallback), so date/datetime
 * columns are not offered where a text column is required.
 */
export function isStringDtype(dtype: string | null | undefined): boolean {
  return dtype != null && STRING_ALIASES.has(dtype.toLowerCase());
}
