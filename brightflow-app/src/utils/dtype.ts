/**
 * Predicates over the column's logical type, shared by every component that
 * branches on type.
 *
 * `LogicalType` is the contract crate's ten-value vocabulary, generated from
 * Rust, so there is nothing to normalise: a column is `Integer` or it is not.
 * `normalizeType` folds the ten into the four buckets the filter-operator
 * catalogue is keyed by; temporal types fall in the string bucket there
 * because no temporal operators exist yet. `isStringType` matches `String`
 * only, so a date column is never offered where a text column is required.
 */

import type { LogicalType } from '@/types/generated';

export type NormalizedType = 'int' | 'float' | 'string' | 'boolean';

const FLOAT_TYPES: ReadonlySet<LogicalType> = new Set<LogicalType>(['Float', 'Decimal']);
const TEMPORAL_TYPES: ReadonlySet<LogicalType> = new Set<LogicalType>([
  'Date',
  'Time',
  'DateTime',
  'DateTimeTz',
]);

/** Fold a logical type into the operator catalogue's buckets; missing falls back to string. */
export function normalizeType(datatype: LogicalType | null | undefined): NormalizedType {
  if (datatype === 'Integer') {
    return 'int';
  }
  if (datatype != null && FLOAT_TYPES.has(datatype)) {
    return 'float';
  }
  if (datatype === 'Boolean') {
    return 'boolean';
  }
  return 'string';
}

/** Integer, Float or Decimal. */
export function isNumericType(datatype: LogicalType | null | undefined): boolean {
  const t = normalizeType(datatype);
  return t === 'int' || t === 'float';
}

/** Float or Decimal — used to pick decimal formatting. */
export function isFloatType(datatype: LogicalType | null | undefined): boolean {
  return datatype != null && FLOAT_TYPES.has(datatype);
}

/** Date, Time, DateTime or DateTimeTz. */
export function isTemporalType(datatype: LogicalType | null | undefined): boolean {
  return datatype != null && TEMPORAL_TYPES.has(datatype);
}

/** `String` only, so a temporal column is not offered where text is required. */
export function isStringType(datatype: LogicalType | null | undefined): boolean {
  return datatype === 'String';
}
