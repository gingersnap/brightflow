/**
 * Aggregation functions for group by
 */
import type { AggregationDef, AggregationOption } from '@/types';

type NormalizedType = 'string' | 'int' | 'float';

const AGGREGATIONS: Record<string, AggregationDef> = {
  count: { label: 'Count', description: 'Count of rows', types: ['*'], usesStar: true },
  sum: { label: 'Sum', description: 'Sum of values', types: ['int', 'float'] },
  avg: { label: 'Average', description: 'Average of values', types: ['int', 'float'] },
  min: { label: 'Min', description: 'Minimum value', types: ['int', 'float', 'string'] },
  max: { label: 'Max', description: 'Maximum value', types: ['int', 'float', 'string'] },
  median: { label: 'Median', description: 'Median value', types: ['int', 'float'] },
  std: { label: 'Std Dev', description: 'Standard deviation', types: ['int', 'float'] },
  first: { label: 'First', description: 'First value in group', types: ['*'] },
  last: { label: 'Last', description: 'Last value in group', types: ['*'] },
};

function normalizeType(dtype: string | null | undefined): NormalizedType {
  if (!dtype) return 'string';
  const t = dtype.toLowerCase();
  if (['int', 'integer', 'bigint', 'i64', 'i32'].includes(t)) return 'int';
  if (['float', 'double', 'decimal', 'f64', 'f32'].includes(t)) return 'float';
  if (['string', 'str', 'text', 'varchar', 'utf8'].includes(t)) return 'string';
  return 'string';
}

export function useAggregations() {
  /**
   * Get available aggregations for a column type
   */
  function getAggregationsForType(dtype: string | null | undefined): AggregationOption[] {
    const normalizedType = normalizeType(dtype);

    return Object.entries(AGGREGATIONS)
      .filter(([_key, agg]) => agg.types.includes('*') || agg.types.includes(normalizedType))
      .map(([key, agg]) => ({
        value: key,
        label: agg.label,
        description: agg.description,
        usesStar: agg.usesStar ?? false,
      }));
  }

  /**
   * Get all aggregation functions
   */
  function getAllAggregations(): AggregationOption[] {
    return Object.entries(AGGREGATIONS).map(([key, agg]) => ({
      value: key,
      label: agg.label,
      description: agg.description,
    }));
  }

  /**
   * Get default aggregation for a type
   */
  function getDefaultAggregation(dtype: string | null | undefined): string {
    const normalizedType = normalizeType(dtype);

    if (['int', 'float'].includes(normalizedType)) {
      return 'sum';
    }
    return 'count';
  }

  /**
   * Format aggregation result label
   */
  function formatAggregationLabel(aggFunction: string, columnName: string): string {
    const agg = AGGREGATIONS[aggFunction];
    if (!agg) return `${aggFunction}(${columnName})`;

    if (aggFunction === 'count' && columnName === '*') {
      return 'Count';
    }

    return `${agg.label}(${columnName})`;
  }

  return {
    getAggregationsForType,
    getAllAggregations,
    getDefaultAggregation,
    formatAggregationLabel,
  };
}
