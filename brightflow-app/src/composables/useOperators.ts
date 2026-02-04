/**
 * Filter operators by column type
 */
import type { OperatorDef, Operator } from '@/types'

type NormalizedType = 'string' | 'int' | 'float' | 'boolean'

const OPERATORS: Record<string, OperatorDef> = {
  // Universal operators
  eq: { label: 'equals', types: ['string', 'int', 'float', 'boolean'] },
  ne: { label: 'not equals', types: ['string', 'int', 'float', 'boolean'] },
  isNull: { label: 'is null', types: ['string', 'int', 'float', 'boolean'], noValue: true },
  isNotNull: { label: 'is not null', types: ['string', 'int', 'float', 'boolean'], noValue: true },

  // String operators
  contains: { label: 'contains', types: ['string'] },

  // Numeric operators
  gt: { label: 'greater than', types: ['int', 'float'] },
  gte: { label: 'greater or equal', types: ['int', 'float'] },
  lt: { label: 'less than', types: ['int', 'float'] },
  lte: { label: 'less or equal', types: ['int', 'float'] },

  // Array operator
  in: { label: 'in list', types: ['string', 'int', 'float'], isArray: true }
}

/**
 * Normalize backend dtype to standard type
 */
function normalizeType(dtype: string | null | undefined): NormalizedType {
  if (!dtype) return 'string'

  const t = dtype.toLowerCase()

  if (['int', 'integer', 'bigint', 'i64', 'i32'].includes(t)) {
    return 'int'
  }
  if (['float', 'double', 'decimal', 'f64', 'f32'].includes(t)) {
    return 'float'
  }
  if (['string', 'str', 'text', 'varchar', 'utf8'].includes(t)) {
    return 'string'
  }
  if (['bool', 'boolean'].includes(t)) {
    return 'boolean'
  }

  return 'string'
}

export function useOperators() {
  /**
   * Get available operators for a column type
   */
  function getOperatorsForType(dtype: string | null | undefined): Operator[] {
    const normalizedType = normalizeType(dtype)

    return Object.entries(OPERATORS)
      .filter(([_key, op]) => op.types.includes(normalizedType))
      .map(([key, op]) => ({
        value: key,
        label: op.label,
        noValue: op.noValue ?? false,
        isArray: op.isArray ?? false
      }))
  }

  /**
   * Get operator details
   */
  function getOperator(operatorKey: string): OperatorDef | null {
    return OPERATORS[operatorKey] ?? null
  }

  /**
   * Check if operator requires a value input
   */
  function operatorNeedsValue(operatorKey: string): boolean {
    const op = OPERATORS[operatorKey]
    return op ? !op.noValue : true
  }

  /**
   * Check if operator accepts array values
   */
  function operatorIsArray(operatorKey: string): boolean {
    const op = OPERATORS[operatorKey]
    return op?.isArray ?? false
  }

  /**
   * Get default operator for a type
   */
  function getDefaultOperator(dtype: string | null | undefined): string {
    const normalizedType = normalizeType(dtype)

    switch (normalizedType) {
      case 'string':
        return 'contains'
      case 'int':
      case 'float':
        return 'eq'
      case 'boolean':
        return 'eq'
      default:
        return 'eq'
    }
  }

  return {
    getOperatorsForType,
    getOperator,
    operatorNeedsValue,
    operatorIsArray,
    getDefaultOperator
  }
}
