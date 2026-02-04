/**
 * REST API client
 */
const API_BASE = import.meta.env.VITE_API_BASE || 'http://localhost:8080'

export class ApiError extends Error {
  status: number
  data: unknown

  constructor(message: string, status: number, data: unknown) {
    super(message)
    this.status = status
    this.data = data
  }
}

interface RequestOptions extends Omit<RequestInit, 'body'> {
  body?: unknown
}

async function request<T>(endpoint: string, options: RequestOptions = {}): Promise<T | null> {
  const url = `${API_BASE}${endpoint}`

  const { body, ...restOptions } = options
  const config: RequestInit = {
    headers: {
      'Content-Type': 'application/json',
      ...restOptions.headers
    },
    ...restOptions
  }

  if (body && typeof body === 'object') {
    config.body = JSON.stringify(body)
  }

  const response = await fetch(url, config)

  if (!response.ok) {
    const data = await response.json().catch(() => ({})) as { message?: string }
    throw new ApiError(
      data.message ?? `Request failed: ${response.status}`,
      response.status,
      data
    )
  }

  // Handle empty responses
  const text = await response.text()
  return text ? (JSON.parse(text) as T) : null
}

export const api = {
  get: <T>(endpoint: string, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'GET', ...options }),
  post: <T>(endpoint: string, body?: unknown, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'POST', body, ...options }),
  put: <T>(endpoint: string, body?: unknown, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'PUT', body, ...options }),
  delete: <T>(endpoint: string, options?: RequestOptions): Promise<T | null> =>
    request<T>(endpoint, { method: 'DELETE', ...options })
}

interface Dataset {
  id: string
  name: string
}

interface UploadResponse {
  id: string
  name: string
  row_count: number
}

interface QueryResponse {
  columns: Array<{ name: string; dtype: string }>
  rows: unknown[][]
  row_count: number
  total_rows: number
}

// Dataset-specific API methods
export const datasetApi = {
  list: (): Promise<Dataset[] | null> => api.get<Dataset[]>('/api/datasets'),
  get: (id: string): Promise<Dataset | null> => api.get<Dataset>(`/api/datasets/${id}`),
  upload: async (file: File): Promise<UploadResponse> => {
    const formData = new FormData()
    formData.append('file', file)

    const response = await fetch(`${API_BASE}/api/datasets/upload`, {
      method: 'POST',
      body: formData
    })

    if (!response.ok) {
      const data = await response.json().catch(() => ({})) as { message?: string }
      throw new ApiError(data.message ?? 'Upload failed', response.status, data)
    }

    return response.json() as Promise<UploadResponse>
  },
  delete: (id: string): Promise<unknown> => api.delete(`/api/datasets/${id}`),
  query: (datasetId: string, operations: unknown[]): Promise<QueryResponse | null> =>
    api.post<QueryResponse>('/api/query', { datasetId, operations })
}
