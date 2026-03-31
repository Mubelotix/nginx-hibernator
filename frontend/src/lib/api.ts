// API key management
const API_KEY_STORAGE_KEY = 'hibernator_api_key'

export function getApiKey(): string | null {
  return localStorage.getItem(API_KEY_STORAGE_KEY)
}

export function setApiKey(key: string) {
  localStorage.setItem(API_KEY_STORAGE_KEY, key)
}

export function clearApiKey() {
  localStorage.removeItem(API_KEY_STORAGE_KEY)
}

export function isAuthenticated(): boolean {
  return getApiKey() !== null
}

// Enhanced fetch with API key
export async function apiFetch(url: string, options: RequestInit = {}): Promise<Response> {
  const apiKey = getApiKey()
  
  const headers = new Headers(options.headers)
  if (apiKey) {
    headers.set('x-api-key', apiKey)
  }
  
  const response = await fetch(url, {
    ...options,
    headers,
  })
  
  // If unauthorized, clear the API key and redirect to login
  if (response.status === 401) {
    clearApiKey()
    window.location.href = '/login'
    throw new Error('Unauthorized')
  }
  
  return response
}
