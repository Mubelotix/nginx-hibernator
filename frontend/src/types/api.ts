export type ConnectionResult = 
  | 'MissingHost'
  | 'UnknownSite'
  | 'InvalidUrl'
  | 'Ignored'
  | 'Unproxied'
  | 'ProxySuccess'
  | 'ProxyFailed'
  | 'ProxyTimeout'
  | 'ApiHandled'

export type ServiceState = 'unknown' | 'down' | 'up' | 'starting'

export interface ServiceInfo {
  name: string
  state: ServiceState
  last_changed: number
}

export interface ServiceConfig {
  [key: string]: any
}

export interface ConnectionMetadata {
  request: string[]
  result: ConnectionResult
  service?: string
  is_browser: boolean
  real_ip?: string
  method: string
  url: string
}

export interface HistoryEntry {
  timestamp: number
  request: string[]
  result: ConnectionResult
  service?: string
  is_browser: boolean
  real_ip?: string
  method: string
  url: string
}

export interface StateHistoryEntry {
  timestamp: number
  service: string
  state: ServiceState
}
