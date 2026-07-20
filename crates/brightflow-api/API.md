# Brightflow API

## Overview

Brightflow API is a WebSocket-based analytics backend for exploring data. It loads data into memory using Polars and supports real-time queries via WebSocket or REST API.

**Base URL:** `http://localhost:8080`

---

## REST Endpoints

### Health Check
```
GET /health
```
Returns `OK` if server is running.

---

### List Datasets
```
GET /api/datasets
```
Returns all loaded datasets.

**Response:**
```json
[
  {
    "id": "default",
    "name": "yc-companies.csv",
    "rowCount": 5000,
    "columnCount": 17,
    "loadedAt": "2024-01-14T10:00:00Z"
  }
]
```

---

### Get Dataset Metadata
```
GET /api/datasets/:id
```
Returns column information for a specific dataset.

**Response:**
```json
{
  "id": "default",
  "name": "yc-companies.csv",
  "rowCount": 5000,
  "columnCount": 17,
  "columns": [
    { "name": "id", "dtype": "int" },
    { "name": "name", "dtype": "string" },
    { "name": "website", "dtype": "string" },
    { "name": "batch", "dtype": "string" },
    { "name": "status", "dtype": "string" },
    { "name": "team_size", "dtype": "int" },
    { "name": "industry", "dtype": "string" }
  ]
}
```

---

### Upload CSV
```
POST /api/datasets/upload
Content-Type: multipart/form-data
```
Upload a new CSV file. Returns dataset metadata with assigned ID.

**Form Fields:**
- `file` (required): The CSV file

**Response (201):**
```json
{
  "id": "abc123-uuid",
  "name": "my_data.csv",
  "rowCount": 1500,
  "columnCount": 12,
  "columns": [...]
}
```

---

### Delete Dataset
```
DELETE /api/datasets/:id
```
Remove a dataset from memory. Cannot delete `default` dataset.

**Response:** `204 No Content`

---

### Execute Query (REST)
```
POST /api/query
Content-Type: application/json
```
Execute a query and return results. Use this for simple one-off queries.

**Request:**
```json
{
  "datasetId": "default",
  "operations": [
    { "type": "filter", "column": "status", "op": "eq", "value": "Active" },
    { "type": "select", "columns": ["name", "batch", "industry"] },
    { "type": "limit", "n": 100 }
  ]
}
```

**Response:**
```json
{
  "columns": [
    { "name": "name", "dtype": "string" },
    { "name": "batch", "dtype": "string" },
    { "name": "industry", "dtype": "string" }
  ],
  "rows": [
    ["Stripe", "S10", "B2B"],
    ["Airbnb", "W09", "Consumer"]
  ],
  "rowCount": 100,
  "totalRows": 5000,
  "executionTimeMs": 12.5
}
```

---

### Curation Actions

One dispatch path for humans and agents. See `src/actions/types.rs` for the
full `Action` union and `GET /api/actions/manifest` for the machine-readable
catalog (kind, description, undoability, JSON Schema per action).

```
POST /api/actions                  { action, requestId }  — idempotent on requestId
GET  /api/actions?limit=100        reverse-chronological audit feed
GET  /api/actions/manifest         action catalog (also the LLM tool registry)
GET  /api/actions/pending-count    proposals awaiting review
POST /api/actions/approve-all      apply every pending proposal, oldest first
POST /api/actions/:id/approve      execute one proposed action
POST /api/actions/:id/reject
POST /api/actions/:id/undo         apply the stored inverse (undoable actions only)
```

Human actions apply immediately. Agent actions are reversibility-tiered by
the run's `mode` (below): auto-apply mode applies undoable kinds immediately
(undo captured at apply time, same as human actions); anything irreversible —
and every action in propose mode — queues as `proposed`.

---

### Agent Runs

```
POST /api/agent/runs               start a run (409 if one is active for the scope)
GET  /api/agent/runs?limit=50
GET  /api/agent/runs/:id           run + the action ids it recorded
POST /api/agent/runs/:id/cancel
POST /api/agent/runs/:id/undo-all  revert every applied, undoable action, newest first
```

**Start request:**
```json
{
  "kind": "auto_label",
  "sourceId": "github",
  "table": "issues",
  "mode": "auto_apply"
}
```
`kind`: `auto_label` | `propose_merges` | `narrate_insights` | `triage_insights`
| `propose_taxonomy` | `label_documents`.
`mode` (optional): `auto_apply` (default) or `propose`. Every tool the runner
hands out is undoable, so auto-apply relies on reversibility instead of
pre-approval.

**Undo-all response:**
```json
{ "total": 12, "undone": 11, "failed": 1, "failures": [ { "logId": 42, "actionKind": "define_taxonomy_category", "error": "..." } ] }
```
Undo-all continues past per-row failures (inverses are blind to interleaved
edits); a partial result leaves the run half-reverted and the failure list is
the record of what remains.

---

## WebSocket API

### Connect
```
ws://localhost:8080/api/ws
```

On connection, server sends:
```json
{ "type": "connected", "serverVersion": "0.1.0" }
```

---

### Message Protocol

All messages are JSON with a `type` field.

#### Client → Server Messages

**Query**
```json
{
  "type": "query",
  "datasetId": "default",
  "operations": [...]
}
```

**Ping** (keepalive)
```json
{ "type": "ping" }
```

**Get Metadata**
```json
{ "type": "getMetadata", "datasetId": "default" }
```

**List Datasets**
```json
{ "type": "listDatasets" }
```

#### Server → Client Messages

**Query Result**
```json
{
  "type": "queryResult",
  "columns": [...],
  "rows": [...],
  "rowCount": 100,
  "totalRows": 5000,
  "executionTimeMs": 3.2
}
```

**Error**
```json
{
  "type": "error",
  "code": "NOT_FOUND",
  "message": "Dataset 'xyz' not found"
}
```

**Pong**
```json
{ "type": "pong" }
```

**Metadata**
```json
{
  "type": "metadata",
  "datasetId": "default",
  "name": "yc-companies.csv",
  "rowCount": 5000,
  "columns": [...]
}
```

**Dataset List**
```json
{
  "type": "datasetList",
  "datasets": [...]
}
```

#### Curation Event Stream (server-push)

Every connected client also receives live curation events — no subscription
message needed (single-tenant: all clients see all events).

**Action Event** — one action-log row changed (created/applied/failed/rejected/undone)
```json
{
  "type": "actionEvent",
  "entry": { "id": 42, "actionKind": "rename_cluster", "status": "applied", "undoable": true, ... },
  "pendingCount": 3
}
```
`entry` is an `ActionLogEntry` (same shape as `GET /api/actions` rows).
`pendingCount` is the server-computed proposals-awaiting-review count.

**Action Batch** — many rows changed at once (approve-all)
```json
{
  "type": "actionBatch",
  "entries": [...],
  "truncated": false,
  "total": 120,
  "succeeded": 118,
  "failed": 2,
  "pendingCount": 0
}
```
`entries` carries at most the newest 100 rows; when `truncated` is true the
client should refetch `GET /api/actions` instead of replaying entries.

**Agent Run** — a run started or changed status
```json
{
  "type": "agentRun",
  "run": { "id": 7, "kind": "auto_label", "status": "running", ... }
}
```

**Action Resync** — the client fell behind the event buffer; refetch the feed
and pending count
```json
{ "type": "actionResync" }
```

---

## Operations

Operations are applied sequentially to transform the data.

### Filter
Filter rows based on a condition.

```json
{ "type": "filter", "column": "status", "op": "eq", "value": "Active" }
```

**Operators:**
| Op | Description | Value Type |
|----|-------------|------------|
| `eq` | Equal | any |
| `ne` | Not equal | any |
| `gt` | Greater than | number |
| `gte` | Greater or equal | number |
| `lt` | Less than | number |
| `lte` | Less or equal | number |
| `contains` | String contains | string |
| `in` | Value in array | array |
| `isNull` | Is null | (none) |
| `isNotNull` | Is not null | (none) |

**Examples:**
```json
{ "type": "filter", "column": "team_size", "op": "gte", "value": 10 }
{ "type": "filter", "column": "batch", "op": "in", "value": ["W24", "S24", "W23"] }
{ "type": "filter", "column": "website", "op": "isNotNull" }
```

---

### Select
Choose specific columns to return.

```json
{ "type": "select", "columns": ["name", "batch", "industry", "team_size"] }
```

---

### Sort
Sort rows by a column.

```json
{ "type": "sort", "by": "team_size", "descending": true }
```

---

### Limit
Limit the number of rows returned.

```json
{ "type": "limit", "n": 100 }
```

---

### GroupBy
Group rows and compute aggregations.

```json
{
  "type": "groupBy",
  "by": ["industry"],
  "aggs": [
    { "column": "*", "function": "count", "alias": "company_count" },
    { "column": "team_size", "function": "sum", "alias": "total_employees" },
    { "column": "team_size", "function": "avg", "alias": "avg_team_size" }
  ]
}
```

**Aggregation Functions:**
| Function | Description |
|----------|-------------|
| `count` | Count rows (use `"*"` for column) |
| `sum` | Sum values |
| `avg` | Average/mean |
| `min` | Minimum value |
| `max` | Maximum value |
| `median` | Median value |
| `std` | Standard deviation |
| `first` | First value |
| `last` | Last value |

---

### Pivot
Create a pivot table.

```json
{
  "type": "pivot",
  "index": ["batch"],
  "columns": "status",
  "values": "name",
  "agg": "count"
}
```

---

## Example Queries

### Top 10 industries by company count
```json
{
  "operations": [
    { "type": "filter", "column": "status", "op": "eq", "value": "Active" },
    { "type": "groupBy", "by": ["industry"], "aggs": [
      { "column": "*", "function": "count", "alias": "count" }
    ]},
    { "type": "sort", "by": "count", "descending": true },
    { "type": "limit", "n": 10 }
  ]
}
```

### Companies by batch and status (pivot)
```json
{
  "operations": [
    { "type": "pivot", "index": ["batch"], "columns": "status", "values": "id", "agg": "count" },
    { "type": "sort", "by": "batch", "descending": true },
    { "type": "limit", "n": 20 }
  ]
}
```

### Large teams in B2B
```json
{
  "operations": [
    { "type": "filter", "column": "industry", "op": "eq", "value": "B2B" },
    { "type": "filter", "column": "team_size", "op": "gte", "value": 50 },
    { "type": "select", "columns": ["name", "one_liner", "team_size", "batch"] },
    { "type": "sort", "by": "team_size", "descending": true }
  ]
}
```

### Recent batches breakdown
```json
{
  "operations": [
    { "type": "filter", "column": "batch", "op": "in", "value": ["W25", "X25", "S24", "W24"] },
    { "type": "groupBy", "by": ["batch", "status"], "aggs": [
      { "column": "*", "function": "count", "alias": "count" }
    ]},
    { "type": "sort", "by": "batch", "descending": true }
  ]
}
```

---

## Error Codes

| Code | Description |
|------|-------------|
| `BAD_REQUEST` | Invalid request parameters |
| `NOT_FOUND` | Dataset not found |
| `INVALID_QUERY` | Query parsing/validation failed |
| `PARSE_ERROR` | Invalid JSON |
| `POLARS_ERROR` | Query execution failed |
| `INTERNAL_ERROR` | Server error |

---

## YC Dataset Columns

The default YC companies dataset has these columns:

| Column | Type | Description |
|--------|------|-------------|
| `id` | int | Unique identifier |
| `name` | string | Company name |
| `small_logo_thumb_url` | string | Logo image URL |
| `website` | string | Company website |
| `all_locations` | string | Location(s) |
| `long_description` | string | Full description |
| `one_liner` | string | Short description |
| `team_size` | int | Number of employees |
| `industry` | string | Primary industry |
| `subindustry` | string | Sub-industry |
| `launched_at` | int | Unix timestamp |
| `tags` | string | JSON array of tags |
| `tags_highlighted` | string | Highlighted tags |
| `batch` | string | YC batch (e.g., "W24", "S23") |
| `status` | string | Company status |
| `industries` | string | JSON array |
| `regions` | string | JSON array |

---

## Frontend Integration Example

### TypeScript WebSocket Client

```typescript
type WsMessage =
  | { type: 'query'; datasetId?: string; operations: Operation[] }
  | { type: 'ping' }
  | { type: 'getMetadata'; datasetId?: string }
  | { type: 'listDatasets' };

type WsResponse =
  | { type: 'connected'; serverVersion: string }
  | { type: 'queryResult'; columns: Column[]; rows: any[][]; rowCount: number; totalRows: number; executionTimeMs: number }
  | { type: 'error'; code: string; message: string }
  | { type: 'pong' }
  | { type: 'metadata'; datasetId: string; name: string; rowCount: number; columns: Column[] }
  | { type: 'datasetList'; datasets: Dataset[] };

class BrightflowClient {
  private ws: WebSocket;
  private pending = new Map<number, { resolve: Function; reject: Function }>();
  private messageId = 0;

  connect(url = 'ws://localhost:8080/api/ws'): Promise<void> {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(url);
      this.ws.onopen = () => resolve();
      this.ws.onerror = reject;
      this.ws.onmessage = (e) => this.handleMessage(JSON.parse(e.data));
    });
  }

  query(operations: Operation[], datasetId = 'default'): Promise<QueryResult> {
    return this.send({ type: 'query', datasetId, operations });
  }

  getMetadata(datasetId = 'default'): Promise<Metadata> {
    return this.send({ type: 'getMetadata', datasetId });
  }

  listDatasets(): Promise<Dataset[]> {
    return this.send({ type: 'listDatasets' });
  }

  private send(msg: WsMessage): Promise<any> {
    this.ws.send(JSON.stringify(msg));
    // Note: This simplified example doesn't track request/response pairs
    // In production, you'd want request IDs or a queue
  }

  private handleMessage(msg: WsResponse) {
    // Handle incoming messages
  }
}
```

### Usage
```typescript
const client = new BrightflowClient();
await client.connect();

// Get top industries
const result = await client.query([
  { type: 'groupBy', by: ['industry'], aggs: [{ column: '*', function: 'count', alias: 'count' }] },
  { type: 'sort', by: 'count', descending: true },
  { type: 'limit', n: 10 }
]);

console.log(result.rows);
```
