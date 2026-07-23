/**
 * Shared display formatting for numbers, periods, columns, and engine errors.
 *
 * Everything here is presentation-only: the raw values stay untouched in the
 * stores; these helpers run at render time.
 */

const numberFormat = new Intl.NumberFormat('en-US', { maximumFractionDigits: 2 });
const intFormat = new Intl.NumberFormat('en-US', { maximumFractionDigits: 0 });
const compactFormat = new Intl.NumberFormat('en-US', {
  notation: 'compact',
  maximumFractionDigits: 1,
});

/** Thousands separators + magnitude-aware precision: 1,234,567 / 12.34 / 0.0042. */
export function formatNumber(value: number): string {
  if (!Number.isFinite(value)) {
    return String(value);
  }
  const abs = Math.abs(value);
  if (abs >= 1000) {
    return intFormat.format(value);
  }
  if (abs > 0 && abs < 0.01) {
    return value.toPrecision(2);
  }
  return numberFormat.format(value);
}

/** Compact form for axis labels: 1.2M, 45K. */
export function formatCompact(value: number): string {
  if (!Number.isFinite(value)) {
    return String(value);
  }
  return compactFormat.format(value);
}

/** Percent with one decimal, sign preserved: "12.3%". */
export function formatPercent(value: number): string {
  if (!Number.isFinite(value)) {
    return String(value);
  }
  return `${numberFormat.format(value)}%`;
}

const MONTH_NAMES = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
] as const;

const MONTH_NAMES_SHORT = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
] as const;

function monthName(monthNum: string, short: boolean): string | null {
  const idx = Math.trunc(Number(monthNum)) - 1;
  const names = short ? MONTH_NAMES_SHORT : MONTH_NAMES;
  return idx >= 0 && idx < names.length ? (names[idx] ?? null) : null;
}

/**
 * Period label → natural language. Port of the Rust `humanize_period`
 * (engine `tree.rs`) so cards and charts read the same:
 * "2023-W12" → "week 12 of 2023", "2023-03" → "March 2023".
 */
export function humanizePeriod(period: string): string {
  if (period.includes('-W')) {
    const [year, week] = period.split('-W');
    if (year != null && week != null) {
      return `week ${week} of ${year}`;
    }
  } else if (period.includes('-Q')) {
    const [year, quarter] = period.split('-Q');
    if (year != null && quarter != null) {
      return `Q${quarter} ${year}`;
    }
  } else if (period.length === 7 && period.includes('-')) {
    const [year, month] = period.split('-');
    if (year != null && month != null) {
      const name = monthName(month, false);
      if (name != null) {
        return `${name} ${year}`;
      }
    }
  }
  return period;
}

/** Compact variant for chart axes: "2023-W12" → "W12 '23", "2023-03" → "Mar '23". */
export function humanizePeriodShort(period: string): string {
  if (period.includes('-W')) {
    const [year, week] = period.split('-W');
    if (year != null && week != null) {
      return `W${week} '${year.slice(-2)}`;
    }
  } else if (period.includes('-Q')) {
    const [year, quarter] = period.split('-Q');
    if (year != null && quarter != null) {
      return `Q${quarter} '${year.slice(-2)}`;
    }
  } else if (period.length === 7 && period.includes('-')) {
    const [year, month] = period.split('-');
    if (year != null && month != null) {
      const name = monthName(month, true);
      if (name != null) {
        return `${name} '${year.slice(-2)}`;
      }
    }
  }
  return period;
}

/** Column name → title-cased label: "order_total" → "Order Total". */
export function humanizeColumn(name: string): string {
  return name
    .replaceAll('_', ' ')
    .split(/\s+/u)
    .filter((word) => word.length > 0)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ');
}

/** Null/empty segment values render as an explicit placeholder, not "". */
export function displaySegmentValue(value: string): string {
  return value === '' ? '(empty)' : value;
}

export interface FriendlyError {
  /** One-sentence, non-technical explanation. */
  message: string;
  /** The raw backend error, preserved for a details disclosure. */
  detail: string | null;
}

/** Map known backend error prefixes to friendly copy; keep the raw detail. */
export function friendlyEngineError(raw: string): FriendlyError {
  if (raw.startsWith('Schema build failed')) {
    return {
      message:
        'The analysis could not read this table’s column types. Check the column settings for this table and try again.',
      detail: raw,
    };
  }
  if (raw.includes('not found in store') || /^Table '.+' not found/u.test(raw)) {
    return {
      message: 'This table has no stored data yet. Run a sync for its source, then try again.',
      detail: raw,
    };
  }
  if (raw.includes('has no data files')) {
    return {
      message: 'This table exists but has no data yet. Run a sync for its source, then try again.',
      detail: raw,
    };
  }
  if (raw.startsWith('No data store configured')) {
    return {
      message:
        'The server is running without a data store, so analyses cannot read any tables. Start the backend with a store directory configured.',
      detail: raw,
    };
  }
  return { message: 'The analysis failed unexpectedly.', detail: raw };
}
