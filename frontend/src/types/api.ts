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
