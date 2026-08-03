/**
 * Tagged console logger that compiles out in production.
 *
 * Non-DEV builds bind every level to a no-op, so debug logging can be left in
 * place without shipping it — and without each call site guarding on an env
 * check.
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
