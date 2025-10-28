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
}

export interface HistoryEntry {
  timestamp: number
  request: string[]
  result: ConnectionResult
  service?: string
}
