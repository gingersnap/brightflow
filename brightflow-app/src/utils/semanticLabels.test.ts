/**
 * Tests for the provenance attribution line: one phrasing per layer, the
 * producer's name and version shown for a declaration.
 */

import { describe, expect, test } from 'vitest';

import { provenanceLabel } from './semanticLabels';

describe('provenanceLabel', () => {
  test('phrases each layer', () => {
    expect(provenanceLabel({ layer: 'user', producer: 'user:1' })).toBe('Edited by a person');
    expect(provenanceLabel({ layer: 'agent', producer: 'agent:7' })).toBe('Set by an agent run');
    expect(provenanceLabel({ layer: 'detected', producer: 'detector' })).toBe(
      'Detected from the data',
    );
  });

  test('a declaration names its producer and version', () => {
    expect(
      provenanceLabel({ layer: 'declared', producer: 'connector:github', version: '0.3.0' }),
    ).toBe('From connector github 0.3.0');
    expect(provenanceLabel({ layer: 'declared', producer: 'enrichment:classify' })).toBe(
      'From enrichment classify',
    );
    expect(provenanceLabel({ layer: 'declared', producer: 'legacy' })).toBe('From legacy');
  });
});
