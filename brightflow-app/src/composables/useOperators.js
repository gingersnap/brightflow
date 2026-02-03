/**
 * Filter operators by column type
 */

const OPERATORS = {
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

export function useOperators() {
  /**
   * Get available operators for a column type
   */
  function getOperatorsForType(dtype) {
    const normalizedType = normalizeType(dtype)

    return Object.entries(OPERATORS)
      .filter(([_, op]) => op.types.includes(normalizedType))
      .map(([key, op]) => ({
        value: key,
        label: op.label,
        noValue: op.noValue || false,
        isArray: op.isArray || false
      }))
  }

  /**
   * Get operator details
   */
  function getOperator(operatorKey) {
    return OPERATORS[operatorKey] || null
  }

  /**
   * Check if operator requires a value input
   */
  function operatorNeedsValue(operatorKey) {
    const op = OPERATORS[operatorKey]
    return op ? !op.noValue : true
  }

  /**
   * Check if operator accepts array values
   */
  function operatorIsArray(operatorKey) {
    const op = OPERATORS[operatorKey]
    return op?.isArray || false
  }

  /**
   * Get default operator for a type
   */
  function getDefaultOperator(dtype) {
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

/**
 * Normalize backend dtype to standard type
 */
function normalizeType(dtype) {
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
