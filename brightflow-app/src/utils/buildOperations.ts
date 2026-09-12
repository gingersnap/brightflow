/**
 * The pivot builder's translation from bucket state to the wire operation
 * chain. Pure, so the table and pivot paths cannot drift and the chain can
 * be tested without a connection.
 *
 * A time field with a granularity is bucketed first: one `withColumns`
 * operation adds every derived period column (named `column__granularity`),
 * and the group-by / pivot then keys on the derived names. Filters come
 * before the bucketing so they apply to raw values; sort and limit come
 * after the aggregate.
 */

import { filterOperations } from '@/stores/query';
import type { AggFn, Filter, PivotField } from '@/types';
import type { DerivedColumn, Operation } from '@/types/generated';

export interface PivotQueryState {
  rowFields: PivotField[];
  columnFields: PivotField[];
  valueFields: PivotField[];
  filters: Filter[];
  filtersEnabled: boolean;
  sortEnabled: boolean;
  sortBy: string | null;
  sortDescending: boolean;
  limitEnabled: boolean;
  limit: number;
}

/** The result-set column a header field groups on: its period column when bucketed. */
export function fieldKey(field: PivotField): string {
  return field.granularity == null ? field.column : `${field.column}__${field.granularity}`;
}

function derivedColumns(fields: PivotField[]): DerivedColumn[] {
  const out: DerivedColumn[] = [];
  for (const field of fields) {
    if (field.granularity != null) {
      out.push({
        expr: { column: field.column, fn: 'period', granularity: field.granularity },
        name: fieldKey(field),
      });
    }
  }
  return out;
}

export function buildPivotOperations(state: PivotQueryState): Operation[] {
  const ops: Operation[] = [];
  if (state.filtersEnabled) {
    ops.push(...filterOperations(state.filters));
  }

  const valueField = state.valueFields[0];
  if (valueField == null) {
    return ops;
  }
  const rowKeys = state.rowFields.map(fieldKey);
  const columnField = state.columnFields[0];
  const columnKey = columnField == null ? null : fieldKey(columnField);
  if (rowKeys.length === 0 && columnKey == null) {
    return ops;
  }

  const derived = derivedColumns([...state.rowFields, ...state.columnFields]);
  if (derived.length > 0) {
    ops.push({ columns: derived, type: 'withColumns' });
  }

  const aggFunc: AggFn = valueField.aggregation ?? 'count';
  const aggs = [{ alias: aggFunc, column: valueField.column, function: aggFunc }];
  if (columnKey == null) {
    ops.push({ aggs, by: rowKeys, type: 'groupBy' });
  } else if (rowKeys.length === 0) {
    ops.push({ aggs, by: [columnKey], type: 'groupBy' });
  } else {
    ops.push({
      agg: aggFunc,
      columns: columnKey,
      index: rowKeys,
      type: 'pivot',
      values: valueField.column,
    });
  }

  if (state.sortEnabled && state.sortBy != null) {
    ops.push({ by: state.sortBy, descending: state.sortDescending, type: 'sort' });
  }
  if (state.limitEnabled && state.limit > 0) {
    ops.push({ n: state.limit, type: 'limit' });
  }
  return ops;
}
