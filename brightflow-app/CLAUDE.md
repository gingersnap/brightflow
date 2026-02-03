# Brightflow App

Vue 3 web application for Brightflow analytics platform.

Current features:
- **Explore** - Interactive data exploration via Rust+Polars backend

## Stack

- **Vite + Vue 3** (JavaScript only, no TypeScript)
- **Nuxt UI 4** as pure Vue (not Nuxt framework) - see `vite.config.js` and `main.js` for setup
- **Tailwind CSS 4** (CSS-first config, no tailwind.config.js)
- **Pinia** for state management
- **vue-echarts** for charts

## Architecture

**Single dataset focus** - no workspace switching, one data source at a time.

**WebSocket for queries** - REST felt too slow for interactive exploration. Connection managed in `stores/connection.js`, query execution in `composables/useQuery.js`.

**Query builder as primary UX** - users build queries visually rather than writing code. Each section (filter, group by, sort, limit) is toggleable. See `stores/query.js` for state shape and `components/query-builder/` for UI.

## Key Files

- `API.md` - Backend API documentation
- `stores/query.js` - Query state and operations builder
- `composables/useOperators.js` - Filter operators by column type
- `services/websocket.js` - WebSocket client with reconnection

## Nuxt UI Notes

Components use Nuxt UI 4 conventions:
- `UTable` uses TanStack Table format (`data` + `columns` with `accessorKey`)
- `USelectMenu` uses `items` prop (not `options`)
- Colors configured in vite plugin, not CSS variables
