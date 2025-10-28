<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import type { HistoryEntry, ConnectionResult } from '@/types/api'

const router = useRouter()
const route = useRoute()

const entries = ref<HistoryEntry[]>([])
const loading = ref(true)
const error = ref<string | null>(null)
const selectedEntry = ref<HistoryEntry | null>(null)

const fetchHistory = async (before?: number, after?: number, updateUrl = true) => {
  try {
    loading.value = true
    error.value = null
    
    let url = '/hibernator-api/history'
    const params = new URLSearchParams()
    
    if (before !== undefined) {
      params.append('before', before.toString())
    }
    if (after !== undefined) {
      params.append('after', after.toString())
    }
    
    if (params.toString()) {
      url += `?${params.toString()}`
    }
    
    const response = await fetch(url)
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`)
    }
    const data = await response.json()
    entries.value = data
    
    // Update URL query parameters
    if (updateUrl) {
      const routeQuery: Record<string, string> = {}
      if (before !== undefined) {
        routeQuery.before = before.toString()
      }
      if (after !== undefined) {
        routeQuery.after = after.toString()
      }
      
      router.push({ query: routeQuery })
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to fetch data'
    console.error('Failed to fetch history:', e)
  } finally {
    loading.value = false
  }
}

const showNextButton = computed(() => {
  return entries.value.length >= 10
})

const showBackButton = computed(() => {
  // Show back button if we have pagination parameters in the URL
  return route.query.before !== undefined || route.query.after !== undefined
})

const loadNext = () => {
  if (entries.value.length === 0) return
  
  // Get the earliest (minimum) timestamp from displayed entries
  const earliestTimestamp = Math.min(...entries.value.map(e => e.timestamp))
  fetchHistory(earliestTimestamp, undefined)
}

const loadBack = () => {
  // Use browser's back button to navigate to previous page
  router.back()
}

const refresh = () => {
  fetchHistory()
}

// Load data based on URL parameters on mount and when route changes
onMounted(() => {
  const before = route.query.before ? Number(route.query.before) : undefined
  const after = route.query.after ? Number(route.query.after) : undefined
  
  if (before !== undefined || after !== undefined) {
    fetchHistory(before, after, false)
  } else {
    fetchHistory()
  }
})

// Watch for route changes (browser back/forward navigation)
watch(() => route.query, (newQuery) => {
  const before = newQuery.before ? Number(newQuery.before) : undefined
  const after = newQuery.after ? Number(newQuery.after) : undefined
  
  fetchHistory(before, after, false)
})

const getStatusClass = (result: ConnectionResult) => {
  switch (result) {
    case 'ProxySuccess':
    case 'ApiHandled':
      return 'status-success'
    case 'ProxyFailed':
    case 'ProxyTimeout':
    case 'InvalidUrl':
      return 'status-error'
    case 'MissingHost':
    case 'UnknownSite':
      return 'status-warning'
    default:
      return 'status-neutral'
  }
}

const getStatusText = (result: ConnectionResult) => {
  switch (result) {
    case 'ProxySuccess':
      return '200'
    case 'ProxyFailed':
      return '502'
    case 'Unproxied':
      return '503'
    case 'ProxyTimeout':
      return '504'
    case 'MissingHost':
    case 'UnknownSite':
      return '404'
    case 'InvalidUrl':
      return '400'
    default:
      return '-'
  }
}

const formatTime = (timestamp: number) => {
  const date = new Date(timestamp * 1000)
  return date.toLocaleTimeString()
}

const selectEntry = (entry: HistoryEntry) => {
  selectedEntry.value = entry
}

const closeSidePanel = () => {
  selectedEntry.value = null
}
</script>

<template>
  <div class="logs-container">
    <div class="logs-header">
      <h2>Request Logs</h2>
      <button @click="refresh" class="refresh-button">Refresh</button>
    </div>

    <div v-if="loading" class="loading">Loading...</div>
    <div v-else-if="error" class="error">Error: {{ error }}</div>
    <div v-else>
      <div class="table-wrapper">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead class="w-[100px]">Time</TableHead>
              <TableHead class="w-[80px]">Method</TableHead>
              <TableHead class="w-[400px]">URL</TableHead>
              <TableHead class="w-[120px]">IP</TableHead>
              <TableHead class="w-[80px]">Status</TableHead>
              <TableHead class="w-[100px]">Result</TableHead>
              <TableHead class="w-[120px]">Service</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow 
              v-for="(entry, index) in entries" 
              :key="index" 
              class="request-row"
              @click="selectEntry(entry)"
            >
              <TableCell class="font-mono text-xs">{{ formatTime(entry.timestamp) }}</TableCell>
              <TableCell class="font-mono method-cell">
                {{ entry.method }}
                <span v-if="entry.is_browser" class="browser-badge" title="Browser Request">👤</span>
                <span v-else class="browser-badge" title="Non-Browser Request">🤖</span>
              </TableCell>
              <TableCell class="font-mono text-xs url-cell" :title="entry.url">
                {{ entry.url }}
              </TableCell>
              <TableCell class="font-mono text-xs real-ip-cell">
                {{ entry.real_ip || '-' }}
              </TableCell>
              <TableCell class="font-mono" :class="getStatusClass(entry.result)">
                {{ getStatusText(entry.result) }}
              </TableCell>
              <TableCell class="font-mono text-xs">{{ entry.result }}</TableCell>
              <TableCell class="text-xs">{{ entry.service || '-' }}</TableCell>
            </TableRow>
          </TableBody>
        </Table>
      </div>
      
      <div class="pagination">
        <button 
          v-if="showBackButton" 
          @click="loadBack" 
          class="nav-button back-button"
        >
          ← Back
        </button>
        <button 
          v-if="showNextButton" 
          @click="loadNext" 
          class="nav-button next-button"
        >
          Next →
        </button>
      </div>
    </div>

    <!-- Side Panel -->
    <div v-if="selectedEntry" class="side-panel-overlay" @click="closeSidePanel">
      <div class="side-panel" @click.stop>
        <div class="side-panel-header">
          <h2>Request Details</h2>
          <button @click="closeSidePanel" class="close-button">×</button>
        </div>
        
        <div class="side-panel-content">
          <!-- Request Line -->
          <div class="section">
            <h3>Request Line</h3>
            <div class="request-line">
              {{ selectedEntry.method }} {{ selectedEntry.url }}
            </div>
          </div>

          <!-- Headers -->
          <div class="section">
            <h3>Headers</h3>
            <div class="headers-list">
              <div 
                v-for="(header, index) in selectedEntry.request" 
                :key="index"
                class="header-item"
              >
                {{ header }}
              </div>
            </div>
          </div>

          <!-- Metadata -->
          <div class="section">
            <h3>Metadata</h3>
            <div class="metadata">
              <div class="metadata-item">
                <span class="metadata-label">Timestamp:</span>
                <span class="metadata-value">{{ new Date(selectedEntry.timestamp * 1000).toLocaleString() }}</span>
              </div>
              <div class="metadata-item" v-if="selectedEntry.real_ip">
                <span class="metadata-label">Original IP:</span>
                <span class="metadata-value real-ip-highlight">{{ selectedEntry.real_ip }}</span>
              </div>
              <div class="metadata-item">
                <span class="metadata-label">Browser Request:</span>
                <span class="metadata-value">
                  {{ selectedEntry.is_browser ? 'Yes 👤' : 'No 🤖' }}
                </span>
              </div>
              <div class="metadata-item">
                <span class="metadata-label">Result:</span>
                <span class="metadata-value" :class="getStatusClass(selectedEntry.result)">{{ selectedEntry.result }}</span>
              </div>
              <div class="metadata-item">
                <span class="metadata-label">Status Code:</span>
                <span class="metadata-value" :class="getStatusClass(selectedEntry.result)">{{ getStatusText(selectedEntry.result) }}</span>
              </div>
              <div class="metadata-item" v-if="selectedEntry.service">
                <span class="metadata-label">Service:</span>
                <span class="metadata-value">{{ selectedEntry.service }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.logs-container {
  width: 100%;
  max-width: 100%;
  padding: 20px;
  background: #ffffff;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
}

.logs-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid #e5e7eb;
}

.logs-header h2 {
  font-size: 20px;
  font-weight: 600;
  color: #1f2937;
  margin: 0;
}

.refresh-button {
  padding: 6px 12px;
  background: #3b82f6;
  color: white;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 13px;
  font-weight: 500;
}

.refresh-button:hover {
  background: #2563eb;
}

.loading,
.error {
  padding: 20px;
  text-align: center;
  color: #6b7280;
}

.error {
  color: #dc2626;
}

.table-wrapper {
  border: 1px solid #e5e7eb;
  border-radius: 4px;
  overflow: hidden;
  background: #ffffff;
}

.pagination {
  display: flex;
  justify-content: center;
  gap: 12px;
  padding: 16px 0;
  margin-top: 12px;
}

.nav-button {
  padding: 8px 16px;
  background: #3b82f6;
  color: white;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 13px;
  font-weight: 500;
  transition: background-color 0.2s;
}

.nav-button:hover {
  background: #2563eb;
}

.nav-button:disabled {
  background: #9ca3af;
  cursor: not-allowed;
}

:deep(.request-row) {
  border-bottom: 1px solid #f3f4f6;
  transition: background-color 0.1s;
  cursor: pointer;
}

:deep(.request-row:hover) {
  background-color: #f9fafb;
}

:deep(thead) {
  background-color: #f9fafb;
  border-bottom: 1px solid #e5e7eb;
}

:deep(th) {
  font-size: 11px;
  font-weight: 600;
  color: #6b7280;
  text-transform: uppercase;
  letter-spacing: 0.025em;
  padding: 8px 12px;
  text-align: left;
}

:deep(td) {
  padding: 6px 12px;
  font-size: 12px;
  color: #1f2937;
}

.method-cell {
  font-weight: 600;
  color: #2563eb;
}

.browser-badge {
  display: inline-block;
  margin-left: 4px;
  font-size: 14px;
  vertical-align: middle;
}

.url-cell {
  max-width: 400px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #1f2937;
}

.real-ip-cell {
  font-weight: 600;
  color: #7c3aed;
  background-color: #f5f3ff;
}

.status-success {
  color: #059669;
  font-weight: 600;
}

.status-error {
  color: #dc2626;
  font-weight: 600;
}

.status-warning {
  color: #d97706;
  font-weight: 600;
}

.status-neutral {
  color: #6b7280;
}

.font-mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace;
}

/* Side Panel Styles */
.side-panel-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.5);
  display: flex;
  justify-content: flex-end;
  z-index: 1000;
  animation: fadeIn 0.2s ease;
}

@keyframes fadeIn {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

.side-panel {
  width: 600px;
  max-width: 90vw;
  background: #ffffff;
  box-shadow: -2px 0 8px rgba(0, 0, 0, 0.1);
  display: flex;
  flex-direction: column;
  animation: slideIn 0.2s ease;
}

@keyframes slideIn {
  from {
    transform: translateX(100%);
  }
  to {
    transform: translateX(0);
  }
}

.side-panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 16px 20px;
  border-bottom: 1px solid #e5e7eb;
  background: #f9fafb;
}

.side-panel-header h2 {
  font-size: 16px;
  font-weight: 600;
  color: #1f2937;
  margin: 0;
}

.close-button {
  background: none;
  border: none;
  font-size: 28px;
  color: #6b7280;
  cursor: pointer;
  padding: 0;
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  transition: background-color 0.2s;
}

.close-button:hover {
  background-color: #e5e7eb;
  color: #1f2937;
}

.side-panel-content {
  flex: 1;
  overflow-y: auto;
  padding: 20px;
}

.section {
  margin-bottom: 24px;
}

.section h3 {
  font-size: 13px;
  font-weight: 600;
  color: #6b7280;
  text-transform: uppercase;
  letter-spacing: 0.025em;
  margin: 0 0 12px 0;
}

.request-line {
  background: #f9fafb;
  padding: 12px;
  border-radius: 4px;
  border: 1px solid #e5e7eb;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace;
  font-size: 13px;
  color: #1f2937;
  word-break: break-all;
}

.headers-list {
  border: 1px solid #e5e7eb;
  border-radius: 4px;
  overflow: hidden;
}

.header-item {
  padding: 10px 12px;
  border-bottom: 1px solid #f3f4f6;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace;
  font-size: 12px;
  color: #1f2937;
  background: #ffffff;
  word-break: break-all;
}

.header-item:last-child {
  border-bottom: none;
}

.header-item:hover {
  background: #f9fafb;
}

.metadata {
  background: #f9fafb;
  padding: 12px;
  border-radius: 4px;
  border: 1px solid #e5e7eb;
}

.metadata-item {
  display: flex;
  padding: 8px 0;
  border-bottom: 1px solid #e5e7eb;
}

.metadata-item:last-child {
  border-bottom: none;
  padding-bottom: 0;
}

.metadata-item:first-child {
  padding-top: 0;
}

.metadata-label {
  font-weight: 600;
  color: #6b7280;
  font-size: 12px;
  min-width: 120px;
}

.metadata-value {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace;
  font-size: 12px;
  color: #1f2937;
  flex: 1;
}

.real-ip-highlight {
  font-weight: 600;
  color: #7c3aed;
  background-color: #f5f3ff;
  padding: 2px 6px;
  border-radius: 3px;
}
</style>
