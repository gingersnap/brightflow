export interface FieldHint {
  key: string;
  label: string;
  placeholder?: string;
  /** Show this field only when another field's value matches. */
  showWhen?: { key: string; equals: string };
  /** If set, render as a dropdown restricted to these values. */
  options?: string[];
  /** Initial value when creating a new preset (no initialValues provided). */
  default?: string;
  /** Optional explanatory text rendered under the field. */
  helperText?: string;
}

export const CONNECTOR_FIELD_HINTS: Record<string, FieldHint[]> = {
  github: [{ key: 'repo', label: 'Repository', placeholder: 'anthropics/claude-code' }],
  bluesky: [
    {
      key: 'identifier',
      label: 'Login identifier',
      placeholder: 'you.bsky.social',
      helperText: 'The Bluesky account used to authenticate (handle, DID, or email).',
    },
    {
      key: 'mode',
      label: 'Mode',
      options: ['keyword', 'actor'],
      default: 'keyword',
    },
    {
      key: 'query',
      label: 'Query',
      placeholder: 'rust',
      showWhen: { key: 'mode', equals: 'keyword' },
    },
    {
      key: 'lang',
      label: 'Language (optional)',
      placeholder: 'en',
      showWhen: { key: 'mode', equals: 'keyword' },
    },
    {
      key: 'actor',
      label: 'Actor',
      placeholder: 'alice.bsky.social',
      showWhen: { key: 'mode', equals: 'actor' },
    },
  ],
};

export function hintsFor(connectorName: string): FieldHint[] {
  return CONNECTOR_FIELD_HINTS[connectorName] ?? [];
}
