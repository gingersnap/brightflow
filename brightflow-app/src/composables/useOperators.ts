/**
 * Filter operators by column type
 */
import type { FilterOp, Operator, OperatorDef } from '@/types';

type NormalizedType = 'string' | 'int' | 'float' | 'boolean';

// Keyed by FilterOp so the record provably covers every backend operator.
// Adding a FilterOp variant without an entry here is a type error.
const OPERATORS: Record<FilterOp, OperatorDef> = {
  // Universal operators
  eq: { label: 'equals', types: ['string', 'int', 'float', 'boolean'] },
  ne: { label: 'not equals', types: ['string', 'int', 'float', 'boolean'] },
  isNull: { label: 'is null', noValue: true, types: ['string', 'int', 'float', 'boolean'] },
  isNotNull: { label: 'is not null', noValue: true, types: ['string', 'int', 'float', 'boolean'] },

  // String operators
  contains: { label: 'contains', types: ['string'] },

  // Numeric operators
  gt: { label: 'greater than', types: ['int', 'float'] },
  gte: { label: 'greater or equal', types: ['int', 'float'] },
  lt: { label: 'less than', types: ['int', 'float'] },
  lte: { label: 'less or equal', types: ['int', 'float'] },

  // Array operator
  in: { isArray: true, label: 'in list', types: ['string', 'int', 'float'] },
};

/**
 * Normalize backend dtype to standard type
 */
function normalizeType(dtype: string | null | undefined): NormalizedType {
  if (dtype == null || dtype === '') {
    return 'string';
  }

  const t = dtype.toLowerCase();

  if (['int', 'integer', 'bigint', 'i64', 'i32'].includes(t)) {
    return 'int';
  }
  if (['float', 'double', 'decimal', 'f64', 'f32'].includes(t)) {
    return 'float';
  }
  if (['string', 'str', 'text', 'varchar', 'utf8'].includes(t)) {
    return 'string';
  }
  if (['bool', 'boolean'].includes(t)) {
    return 'boolean';
  }

  return 'string';
}

/** Narrow an arbitrary key (e.g. from a select) to a known operator. */
function isFilterOp(key: string): key is FilterOp {
  return key in OPERATORS;
}

export function useOperators() {
  /**
   * Get available operators for a column type
   */
  function getOperatorsForType(dtype: string | null | undefined): Operator[] {
    const normalizedType = normalizeType(dtype);

    return Object.entries(OPERATORS)
      .filter(([_key, op]) => op.types.includes(normalizedType))
      .map(([key, op]) => ({
        isArray: op.isArray ?? false,
        label: op.label,
        noValue: op.noValue ?? false,
        value: key,
      }));
  }

  /**
   * Get operator details
   */
  function getOperator(operatorKey: string): OperatorDef | null {
    return isFilterOp(operatorKey) ? OPERATORS[operatorKey] : null;
  }

  /**
   * Check if operator requires a value input
   */
  function operatorNeedsValue(operatorKey: string): boolean {
    const op = getOperator(operatorKey);
    return op == null ? true : op.noValue !== true;
  }

  /**
   * Check if operator accepts array values
   */
  function operatorIsArray(operatorKey: string): boolean {
    return getOperator(operatorKey)?.isArray ?? false;
  }

  /**
   * Get default operator for a type
   */
  function getDefaultOperator(dtype: string | null | undefined): FilterOp {
    const normalizedType = normalizeType(dtype);

    switch (normalizedType) {
      case 'string': {
        return 'contains';
      }
      case 'int':
      case 'float': {
        return 'eq';
      }
      case 'boolean': {
        return 'eq';
      }
      default: {
        return 'eq';
      }
    }
  }

  return {
    getDefaultOperator,
    getOperator,
    getOperatorsForType,
    isFilterOp,
    operatorIsArray,
    operatorNeedsValue,
  };
}
