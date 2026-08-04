/** Tests for the quote-aware CSV preview and the upload-name slugifier. */

import { describe, expect, test } from 'vitest';

import { previewCsv, slugifyTableName } from './csvPreview';

describe('previewCsv', () => {
  test('splits headers and rows on plain input', () => {
    const preview = previewCsv('a,b\n1,2\n3,4\n');
    expect(preview.headers).toEqual(['a', 'b']);
    expect(preview.rows).toEqual([
      ['1', '2'],
      ['3', '4'],
    ]);
  });

  test('handles quoted fields with commas, escaped quotes, and newlines', () => {
    const preview = previewCsv('name,note\n"Smith, Jane","said ""hi""\nsecond line"\n');
    expect(preview.rows).toEqual([['Smith, Jane', 'said "hi"\nsecond line']]);
  });

  test('handles CRLF line endings and skips the trailing empty record', () => {
    const preview = previewCsv('a,b\r\n1,2\r\n');
    expect(preview.headers).toEqual(['a', 'b']);
    expect(preview.rows).toEqual([['1', '2']]);
  });

  test('stops after maxRows data rows', () => {
    const text = `h\n${Array.from({ length: 10 }, (_, i) => String(i)).join('\n')}\n`;
    const preview = previewCsv(text, 3);
    expect(preview.rows).toEqual([['0'], ['1'], ['2']]);
  });

  test('a final record without a trailing newline is kept', () => {
    expect(previewCsv('a\n1').rows).toEqual([['1']]);
  });
});

describe('slugifyTableName', () => {
  test('lowercases, strips the extension, and collapses separators', () => {
    expect(slugifyTableName('Q3 Leads.csv')).toBe('q3_leads');
    expect(slugifyTableName('My--Report (final).csv')).toBe('my_report_final');
  });

  test('degenerate names fall back safely', () => {
    expect(slugifyTableName('.csv')).toBe('table');
    expect(slugifyTableName('$$$.csv')).toBe('table');
    expect(slugifyTableName('2026 results.csv')).toBe('t_2026_results');
  });
});
