/**
 * Display names for the semantic enums the backend defines (`ColumnRole`,
 * `Polarity`) and the one-line attribution for a `Provenance`. Pure data
 * and pure functions, so both components and composables can read them
 * without pulling in a Vue component; the wire values themselves come from
 * the generated types.
 */

import type { ColumnRole, Polarity, Provenance } from '@/types/generated';

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

/**
 * Who a column's semantics come from, as one line: "Edited by a person",
 * "Set by an agent run", "From connector github 0.3.0", "Detected from the
 * data". The producer is spelled `kind:name`; the name is shown verbatim
 * because the frontend has no display names for connectors.
 */
export function provenanceLabel(provenance: Provenance): string {
  const [kind, ...rest] = provenance.producer.split(':');
  const name = rest.join(':');
  const version = provenance.version == null ? '' : ` ${provenance.version}`;
  switch (provenance.layer) {
    case 'user': {
      return 'Edited by a person';
    }
    case 'agent': {
      return 'Set by an agent run';
    }
    case 'detected': {
      return 'Detected from the data';
    }
    case 'declared': {
      if (kind === 'connector' && name !== '') {
        return `From connector ${name}${version}`;
      }
      if (kind === 'enrichment' && name !== '') {
        return `From enrichment ${name}${version}`;
      }
      return `From ${provenance.producer}${version}`;
    }
    default: {
      return `From ${provenance.producer}`;
    }
  }
}
