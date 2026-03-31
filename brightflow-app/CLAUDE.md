# Brightflow App

Vue 3 web application for Brightflow analytics platform.

Current features:

- **Explore** - Interactive data exploration via Rust+Polars backend

## Stack

- **Vite+ (Vite 8) + Vue 3 + TypeScript** (strict mode)
- **Nuxt UI 4** as pure Vue (not Nuxt framework) - see `vite.config.ts` and `main.ts` for setup
- **Tailwind CSS 4** (CSS-first config, no tailwind.config.js)
- **Pinia** for state management
- **vue-echarts** for charts

## TypeScript

Strict TypeScript is enabled with all strict flags plus additional checks:

- `noUncheckedIndexedAccess` - array/object access returns `T | undefined`
- `exactOptionalPropertyTypes` - distinguishes missing vs undefined
- `noUnusedLocals` / `noUnusedParameters` - errors on dead code

Shared types in `src/types/index.ts`. Generated types from Rust (via ts-rs) in `src/types/generated/`. Run `npm run check` to verify.

## Linting & Formatting

- **Vite+** unified toolchain: Oxlint (linter), Oxfmt (formatter), tsgolint (type checker)
- Formatter config: semicolons: always, trailing commas: all, single quotes, 100 char line width
- All config in `vite.config.ts` under `lint` and `fmt` blocks

Run `npm run check` to verify all (types + lint + format). Run `npm run check:fix` to auto-fix. Run `npm run fmt` to format only.

## Architecture

**Single dataset focus** - no workspace switching, one data source at a time.

**WebSocket for queries** - REST felt too slow for interactive exploration. Connection managed in `stores/connection.ts`, query execution in `composables/useWsQuery.ts`.

**Query builder as primary UX** - users build queries visually rather than writing code. Each section (filter, group by, sort, limit) is toggleable. See `stores/query.ts` for state shape and `components/query/` for UI.

## Key Files

- `../crates/brightflow-api/API.md` - Backend API documentation
- `src/types/index.ts` - Shared TypeScript types (frontend-only + re-exports from generated)
- `src/types/generated/` - TypeScript types auto-generated from Rust via ts-rs
- `stores/query.ts` - Query state and operations builder
- `composables/useOperators.ts` - Filter operators by column type
- `services/websocket.ts` - WebSocket client with reconnection

## Console Forwarding

Vite 8's built-in `server.forwardConsole` forwards browser `console.*` calls to the dev terminal. Configured in `vite.config.ts`. All frontend logs visible in the terminal without browser DevTools. Dev-only, no production impact.

## Nuxt UI Notes

Components use Nuxt UI 4 conventions:

- `UTable` uses TanStack Table format (`data` + `columns` with `accessorKey`)
- `USelectMenu` uses `items` prop (not `options`)
- Colors configured in vite plugin, not CSS variables
