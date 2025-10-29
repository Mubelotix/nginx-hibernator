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
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip'
import type { StateHistoryEntry, ServiceState } from '@/types/api'

const props = defineProps<{
  serviceName?: string
}>()

const router = useRouter()
const route = useRoute()

const entries = ref<StateHistoryEntry[]>([])
const loading = ref(true)
const error = ref<string | null>(null)

const fetchStateHistory = async (before?: number, after?: number, updateUrl = true) => {
  try {
    loading.value = true
    error.value = null
    
    let url = '/hibernator-api/state-history'
    const params = new URLSearchParams()
    
    if (before !== undefined) {
      params.append('before', before.toString())
    }
    if (after !== undefined) {
      params.append('after', after.toString())
    }
    if (props.serviceName) {
      params.append('service', props.serviceName)
    }
    
    if (params.toString()) {
      url += `?${params.toString()}`
    }
    
    const response = await fetch(url)
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`)
    }
    const data = await response.json()

    // Reverse to show newest first
    entries.value = data.reverse()
    
    // Update URL query parameters
    if (updateUrl) {
      const routeQuery: Record<string, string> = {}
      if (before !== undefined) {
        routeQuery.stateBefore = before.toString()
      }
      if (after !== undefined) {
        routeQuery.stateAfter = after.toString()
      }
      
      router.push({ query: { ...route.query, ...routeQuery } })
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to fetch data'
    console.error('Failed to fetch state history:', e)
  } finally {
    loading.value = false
  }
}

const showNextButton = computed(() => {
  return entries.value.length >= 10
})

const showBackButton = computed(() => {
  // Show back button if we have pagination parameters in the URL
  return route.query.stateBefore !== undefined || route.query.stateAfter !== undefined
})

const loadNext = () => {
  if (entries.value.length === 0) return
  
  // Get the earliest (minimum) start_time from displayed entries
  const earliestTimestamp = Math.min(...entries.value.map(e => e.start_time))
  fetchStateHistory(earliestTimestamp, undefined)
}

const loadBack = () => {
  // Use browser's back button to navigate to previous page
  router.back()
}

const refresh = () => {
  fetchStateHistory()
}

// Load data based on URL parameters on mount and when route changes
onMounted(() => {
  const before = route.query.stateBefore ? Number(route.query.stateBefore) : undefined
  const after = route.query.stateAfter ? Number(route.query.stateAfter) : undefined
  
  if (before !== undefined || after !== undefined) {
    fetchStateHistory(before, after, false)
  } else {
    fetchStateHistory()
  }
})

// Watch for route changes (browser back/forward navigation)
watch(() => [route.query.stateBefore, route.query.stateAfter], () => {
  const before = route.query.stateBefore ? Number(route.query.stateBefore) : undefined
  const after = route.query.stateAfter ? Number(route.query.stateAfter) : undefined
  
  fetchStateHistory(before, after, false)
})

const getStateClass = (state: ServiceState) => {
  switch (state) {
    case 'up':
      return 'state-up'
    case 'down':
      return 'state-down'
    case 'starting':
      return 'state-starting'
    default:
      return 'state-unknown'
  }
}

const getStateEmoji = (state: ServiceState) => {
  switch (state) {
    case 'up':
      return '✅'
    case 'down':
      return '💤'
    case 'starting':
      return '⚙️'
    default:
      return '❓'
  }
}

const getStateTooltip = (state: ServiceState) => {
  switch (state) {
    case 'up':
      return 'Service is running and consuming resources. It will be hibernated after a period of inactivity.'
    case 'down':
      return 'Service is down but will start automatically upon request. This keeps the machine completely idle.'
    case 'starting':
      return 'Users are waiting for the service to start at the landing page'
    default:
      return 'Unknown state'
  }
}

const formatTime = (timestamp: number) => {
  const date = new Date(timestamp * 1000)
  return date.toLocaleString()
}

const formatDuration = (startTime: number, endTime: number) => {
  const durationMs = (endTime - startTime) * 1000
  const seconds = Math.floor(durationMs / 1000)
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)

  if (days > 0) {
    return `${days}d ${hours % 24}h`
  } else if (hours > 0) {
    return `${hours}h ${minutes % 60}m`
  } else if (minutes > 0) {
    return `${minutes}m ${seconds % 60}s`
  } else {
    return `${seconds}s`
  }
}
</script>

<template>
  <div class="state-history-container">
    <div class="state-history-header">
      <h2>State History</h2>
      <button @click="refresh" class="refresh-button">Refresh</button>
    </div>

    <div v-if="loading" class="loading">Loading...</div>
    <div v-else-if="error" class="error">Error: {{ error }}</div>
    <div v-else>
      <div class="table-wrapper">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead class="w-[200px]">Start Time</TableHead>
              <TableHead class="w-[200px]">End Time</TableHead>
              <TableHead class="w-[120px]">Duration</TableHead>
              <TableHead v-if="!props.serviceName" class="w-[150px]">Service</TableHead>
              <TableHead class="w-[150px]">State</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow 
              v-for="(entry, index) in entries" 
              :key="index" 
              class="state-row"
            >
              <TableCell class="font-mono text-sm">{{ formatTime(entry.start_time) }}</TableCell>
              <TableCell class="font-mono text-sm">{{ formatTime(entry.end_time) }}</TableCell>
              <TableCell class="font-mono text-sm">{{ formatDuration(entry.start_time, entry.end_time) }}</TableCell>
              <TableCell v-if="!props.serviceName" class="text-sm">{{ entry.service }}</TableCell>
              <TableCell class="font-medium" :class="getStateClass(entry.state)">
                <TooltipProvider>
                  <Tooltip>
                    <TooltipTrigger as-child>
                      <span class="state-badge">
                        {{ getStateEmoji(entry.state) }} {{ entry.state === 'down' ? 'HIBERNATING' : entry.state.toUpperCase() }}
                      </span>
                    </TooltipTrigger>
                    <TooltipContent>
                      <p>{{ getStateTooltip(entry.state) }}</p>
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              </TableCell>
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
  </div>
</template>

<style scoped>
.state-history-container {
  width: 100%;
  max-width: 100%;
  min-height: 100%;
  padding: 20px;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
}

.state-history-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid #e5e7eb;
}

.state-history-header h2 {
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

:deep(.state-row) {
  border-bottom: 1px solid #f3f4f6;
  transition: background-color 0.1s;
}

:deep(.state-row:hover) {
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
  padding: 8px 12px;
  font-size: 12px;
  color: #1f2937;
}

.state-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border-radius: 12px;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.025em;
}

.state-up {
  color: #059669;
}

.state-up .state-badge {
  background: #d1fae5;
}

.state-down {
  color: #2563eb;
}

.state-down .state-badge {
  background: #dbeafe;
}

.state-starting {
  color: #d97706;
}

.state-starting .state-badge {
  background: #fed7aa;
}

.state-unknown {
  color: #6b7280;
}

.state-unknown .state-badge {
  background: #f3f4f6;
}

.font-mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace;
}
</style>
