/**
 * Quote-aware first-rows CSV preview — enough for an upload preview table,
 * deliberately not a full parser (the backend's Polars reader is the truth).
 */

export interface CsvPreview {
  headers: string[];
  rows: string[][];
}

/** Parse the first `maxRows` records (plus header) from CSV text. */
export function previewCsv(text: string, maxRows = 5): CsvPreview {
  const records: string[][] = [];
  let field = '';
  let record: string[] = [];
  let inQuotes = false;

  const pushField = (): void => {
    record.push(field);
    field = '';
  };
  const pushRecord = (): boolean => {
    pushField();
    // Skip fully empty records (trailing newline)
    if (record.length > 1 || (record[0] ?? '') !== '') {
      records.push(record);
    }
    record = [];
    return records.length >= maxRows + 1;
  };

  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (inQuotes) {
      if (ch === '"') {
        if (text[i + 1] === '"') {
          field += '"';
          i++;
        } else {
          inQuotes = false;
        }
      } else {
        field += ch;
      }
    } else if (ch === '"') {
      inQuotes = true;
    } else if (ch === ',') {
      pushField();
    } else if (ch === '\n') {
      if (pushRecord()) {
        return toPreview(records);
      }
    } else if (ch !== '\r') {
      field += ch;
    }
  }
  if (field !== '' || record.length > 0) {
    pushRecord();
  }
  return toPreview(records);
}

function toPreview(records: string[][]): CsvPreview {
  const [headers = [], ...rows] = records;
  return { headers, rows };
}

/** Slugified table name from a filename ("Q3 Leads.csv" → "q3_leads"). */
export function slugifyTableName(filename: string): string {
  const stem = filename.replace(/\.[^.]+$/u, '');
  let slug = stem
    .toLowerCase()
    .replaceAll(/[^a-z0-9]/gu, '_')
    .replaceAll(/_{2,}/gu, '_')
    .replaceAll(/^_+|_+$/gu, '');
  if (slug === '') {
    slug = 'table';
  }
  if (/^\d/u.test(slug)) {
    slug = `t_${slug}`;
  }
  return slug;
}
