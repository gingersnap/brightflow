import type { FilterOp, Operator, OperatorDef } from '@/types';
/**
 * Filter operators by column type
 */
import type { LogicalType } from '@/types/generated';
import { normalizeType } from '@/utils/dtype';

// Keyed by FilterOp so the record provably covers every backend operator.
// Adding a FilterOp variant without an entry here is a type error.
const OPERATORS: Record<FilterOp, OperatorDef> = {
  // Universal operators
  eq: {
    label: 'equals',
    temporalLabel: 'on',
    types: ['string', 'int', 'float', 'boolean', 'temporal'],
  },
  ne: {
    label: 'not equals',
    temporalLabel: 'not on',
    types: ['string', 'int', 'float', 'boolean', 'temporal'],
  },
  isNull: {
    label: 'is null',
    noValue: true,
    types: ['string', 'int', 'float', 'boolean', 'temporal'],
  },
  isNotNull: {
    label: 'is not null',
    noValue: true,
    types: ['string', 'int', 'float', 'boolean', 'temporal'],
  },

  // String operators
  contains: { label: 'contains', types: ['string'] },

  // Numeric and temporal operators. The backend compares a temporal column
  // With a date or timestamp literal, so "after" is `gt` on the wire.
  gt: { label: 'greater than', temporalLabel: 'after', types: ['int', 'float', 'temporal'] },
  gte: {
    label: 'greater or equal',
    temporalLabel: 'on or after',
    types: ['int', 'float', 'temporal'],
  },
  lt: { label: 'less than', temporalLabel: 'before', types: ['int', 'float', 'temporal'] },
  lte: {
    label: 'less or equal',
    temporalLabel: 'on or before',
    types: ['int', 'float', 'temporal'],
  },

  // Array operator
  in: { isArray: true, label: 'in list', types: ['string', 'int', 'float'] },
};

/** Narrow an arbitrary key (e.g. from a select) to a known operator. */
function isFilterOp(key: string): key is FilterOp {
  return key in OPERATORS;
}

/**
 * Get default operator for a type
 */
function getDefaultOperator(datatype: LogicalType | null | undefined): FilterOp {
  const normalizedType = normalizeType(datatype);

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
    case 'temporal': {
      return 'gte';
    }
    default: {
      return 'eq';
    }
  }
}

export function useOperators() {
  /**
   * Get available operators for a column type
   */
  function getOperatorsForType(datatype: LogicalType | null | undefined): Operator[] {
    const normalizedType = normalizeType(datatype);

    return Object.entries(OPERATORS)
      .filter(([_key, op]) => op.types.includes(normalizedType))
      .map(([key, op]) => ({
        isArray: op.isArray ?? false,
        label: normalizedType === 'temporal' ? (op.temporalLabel ?? op.label) : op.label,
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

  return {
    getDefaultOperator,
    getOperator,
    getOperatorsForType,
    isFilterOp,
    operatorIsArray,
    operatorNeedsValue,
  };
}
