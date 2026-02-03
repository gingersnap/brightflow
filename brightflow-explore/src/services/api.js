/**
 * REST API client
 */
const API_BASE = import.meta.env.VITE_API_BASE || 'http://localhost:8080'

class ApiError extends Error {
  constructor(message, status, data) {
    super(message)
    this.status = status
    this.data = data
  }
}

async function request(endpoint, options = {}) {
  const url = `${API_BASE}${endpoint}`

  const config = {
    headers: {
      'Content-Type': 'application/json',
      ...options.headers
    },
    ...options
  }

  if (options.body && typeof options.body === 'object') {
    config.body = JSON.stringify(options.body)
  }

  const response = await fetch(url, config)

  if (!response.ok) {
    const data = await response.json().catch(() => ({}))
    throw new ApiError(
      data.message || `Request failed: ${response.status}`,
      response.status,
      data
    )
  }

  // Handle empty responses
  const text = await response.text()
  return text ? JSON.parse(text) : null
}

export const api = {
  get: (endpoint, options) => request(endpoint, { method: 'GET', ...options }),
  post: (endpoint, body, options) => request(endpoint, { method: 'POST', body, ...options }),
  put: (endpoint, body, options) => request(endpoint, { method: 'PUT', body, ...options }),
  delete: (endpoint, options) => request(endpoint, { method: 'DELETE', ...options })
}

// Dataset-specific API methods
export const datasetApi = {
  list: () => api.get('/api/datasets'),
  get: (id) => api.get(`/api/datasets/${id}`),
  upload: async (file) => {
    const formData = new FormData()
    formData.append('file', file)

    const response = await fetch(`${API_BASE}/api/datasets/upload`, {
      method: 'POST',
      body: formData
    })

    if (!response.ok) {
      const data = await response.json().catch(() => ({}))
      throw new ApiError(data.message || 'Upload failed', response.status, data)
    }

    return response.json()
  },
  delete: (id) => api.delete(`/api/datasets/${id}`),
  query: (datasetId, operations) => api.post('/api/query', { datasetId, operations })
}

export { ApiError }
