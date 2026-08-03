/**
 * Tagged console logger, split by level on whether the build is DEV.
 *
 * `debug` and `info` become no-ops outside DEV, so tracing can be left in place
 * without shipping it and without each call site guarding on an env check.
 * `warn` and `error` always log — a production problem the user is hitting is
 * exactly the thing you need in the console, so don't gate those.
 */

interface Logger {
  debug: (...args: unknown[]) => void;
  info: (...args: unknown[]) => void;
  warn: (...args: unknown[]) => void;
  error: (...args: unknown[]) => void;
}

const noop = (): void => {};

function createLogger(tag: string): Logger {
  const prefix = `[${tag}]`;
  return {
    // oxlint-disable-next-line no-console
    debug: import.meta.env.DEV ? console.debug.bind(console, prefix) : noop,
    // oxlint-disable-next-line no-console
    info: import.meta.env.DEV ? console.info.bind(console, prefix) : noop,
    // oxlint-disable-next-line no-console
    warn: console.warn.bind(console, prefix),
    // oxlint-disable-next-line no-console
    error: console.error.bind(console, prefix),
  };
}

export { createLogger, type Logger };
