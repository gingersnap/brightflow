/**
 * Display names for the semantic enums the backend defines (`ColumnRole`,
 * `Polarity`). Pure data, so both components and composables can read it
 * without pulling in a Vue component; the wire values themselves come from
 * the generated types.
 */

import type { ColumnRole, Polarity } from '@/types/generated';

export const ROLE_LABELS: Record<ColumnRole, string> = {
  dimension: 'Dimension',
  entity: 'Entity',
  ignored: 'Ignored',
  measure: 'Measure',
  time: 'Time',
};

export const POLARITY_LABELS: Record<Polarity, string> = {
  higher_is_better: 'Higher is better',
  lower_is_better: 'Lower is better',
  neutral: 'Neutral',
};
