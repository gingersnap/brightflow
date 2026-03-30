# Brightflow App

Vue 3 web application for Brightflow analytics platform.

Current features:
- **Explore** - Interactive data exploration via Rust+Polars backend

## Stack

- **Vite + Vue 3 + TypeScript** (strict mode)
- **Nuxt UI 4** as pure Vue (not Nuxt framework) - see `vite.config.ts` and `main.ts` for setup
- **Tailwind CSS 4** (CSS-first config, no tailwind.config.js)
- **Pinia** for state management
- **vue-echarts** for charts

## TypeScript

Strict TypeScript is enabled with all strict flags plus additional checks:
- `noUncheckedIndexedAccess` - array/object access returns `T | undefined`
- `exactOptionalPropertyTypes` - distinguishes missing vs undefined
- `noUnusedLocals` / `noUnusedParameters` - errors on dead code

Shared types in `src/types/index.ts`. Generated types from Rust (via ts-rs) in `src/types/generated/`. Run `npm run type-check` to verify.

## Linting & Formatting

- **oxlint** - Fast Rust-based linter with strict categories (correctness, suspicious, pedantic, perf, style)
- **Biome** - Fast Rust-based formatter (semicolons: always, trailing commas: all, single quotes)

Run `npm run check` to verify all (types + lint + format). Run `npm run format` to auto-fix formatting.

## Architecture

**Single dataset focus** - no workspace switching, one data source at a time.

**WebSocket for queries** - REST felt too slow for interactive exploration. Connection managed in `stores/connection.ts`, query execution in `composables/useWsQuery.ts`.

**Query builder as primary UX** - users build queries visually rather than writing code. Each section (filter, group by, sort, limit) is toggleable. See `stores/query.ts` for state shape and `components/query-builder/` for UI.

## Key Files

- `API.md` - Backend API documentation
- `src/types/index.ts` - Shared TypeScript types (frontend-only + re-exports from generated)
- `src/types/generated/` - TypeScript types auto-generated from Rust via ts-rs
- `stores/query.ts` - Query state and operations builder
- `composables/useOperators.ts` - Filter operators by column type
- `services/websocket.ts` - WebSocket client with reconnection

## Console Forwarding

`vite-console-forward-plugin.ts` forwards browser `console.*` calls to the Vite dev terminal (prefixed `[browser]`). This means all frontend logs are visible in the terminal without browser DevTools. Dev-only, no production impact.

## Nuxt UI Notes

Components use Nuxt UI 4 conventions:
- `UTable` uses TanStack Table format (`data` + `columns` with `accessorKey`)
- `USelectMenu` uses `items` prop (not `options`)
- Colors configured in vite plugin, not CSS variables
