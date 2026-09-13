/**
 * The bridge between Explore's builder state and a model's recipe: capture
 * the chain the builder would run (the pivot chain when values are set,
 * the table chain otherwise) together with the view snapshot that produced
 * it, and describe a chain in words for the Semantics page and the model
 * menu. Pure, so the two directions can be tested without a store.
 */

import { filterOperations } from '@/stores/query';
import type { AggSpec, ModelRecipe, Operation } from '@/types/generated';

import { buildPivotOperations } from './buildOperations';
import {
  captureExploreView,
  type ExploreViewSpec,
  type PivotSnapshot,
  type QuerySnapshot,
} from './viewSpec';

export interface CapturedModel {
  recipe: ModelRecipe;
  clientSpec: ExploreViewSpec;
}

/** The chain Explore is showing right now, with the snapshot to reopen it. */
export function captureRecipe(query: QuerySnapshot, pivot: PivotSnapshot): CapturedModel {
  const operations =
    pivot.valueFields.length > 0
      ? buildPivotOperations({
          columnFields: pivot.columnFields,
          filters: query.filters,
          filtersEnabled: query.sections.filter.enabled,
          limit: query.limit,
          limitEnabled: query.sections.limit.enabled,
          rowFields: pivot.rowFields,
          sortBy: query.sortBy,
          sortDescending: query.sortDescending,
          sortEnabled: query.sections.sort.enabled,
          valueFields: pivot.valueFields,
        })
      : tableOperations(query);
  return {
    clientSpec: captureExploreView(query, pivot),
    recipe: { operations, version: 1 },
  };
}

/** The table path: filters, sort and limit, each only when its section is on. */
function tableOperations(query: QuerySnapshot): Operation[] {
  const ops: Operation[] = [];
  if (query.sections.filter.enabled) {
    ops.push(...filterOperations(query.filters));
  }
  if (query.sections.sort.enabled && query.sortBy != null) {
    ops.push({ by: query.sortBy, descending: query.sortDescending, type: 'sort' });
  }
  if (query.sections.limit.enabled && query.limit > 0) {
    ops.push({ n: query.limit, type: 'limit' });
  }
  return ops;
}

function aggText(agg: AggSpec): string {
  return `${agg.function} of ${agg.column === '*' ? 'rows' : agg.column}`;
}

/** One step in words, in the order the chain runs. */
export function describeOperation(op: Operation): string {
  switch (op.type) {
    case 'filter': {
      return `filter ${op.column} ${op.op} ${JSON.stringify(op.value)}`;
    }
    case 'select': {
      return `keep ${op.columns.join(', ')}`;
    }
    case 'withColumns': {
      return op.columns
        .map((c) => `${c.name} = ${c.expr.column} by ${c.expr.granularity}`)
        .join(', ');
    }
    case 'groupBy': {
      const by = op.by.length > 0 ? ` by ${op.by.join(', ')}` : '';
      return `${op.aggs.map((agg) => aggText(agg)).join(', ')}${by}`;
    }
    case 'pivot': {
      return `pivot ${op.values} across ${op.columns} by ${op.index.join(', ')}`;
    }
    case 'sort': {
      return `sort by ${op.by}${op.descending ? ' descending' : ''}`;
    }
    case 'limit': {
      return `top ${op.n}`;
    }
    default: {
      return 'unknown step';
    }
  }
}

/** The whole chain in one line: "filter region eq \"EU\" · sum of revenue by region". */
export function describeOperations(ops: Operation[]): string {
  return ops.length > 0 ? ops.map((op) => describeOperation(op)).join(' · ') : 'the whole table';
}
