<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import type { ServiceInfo } from '@/types/api'

const router = useRouter()
const services = ref<ServiceInfo[]>([])
const loading = ref(true)
const error = ref<string | null>(null)
let refreshInterval: number | null = null

const fetchServices = async () => {
  try {
    error.value = null
    
    const response = await fetch('/hibernator-api/services')
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`)
    }
    const data = await response.json()
    services.value = data
    loading.value = false
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to fetch services'
    console.error('Failed to fetch services:', e)
    loading.value = false
  }
}

const navigateToService = (serviceName: string) => {
  router.push(`/services/${serviceName}`)
}

const getStateClass = (state: string) => {
  switch (state) {
    case 'up':
      return 'state-up'
    case 'down':
      return 'state-down'
    case 'starting':
      return 'state-starting'
    case 'unknown':
    default:
      return 'state-unknown'
  }
}

const getStateIcon = (state: string) => {
  switch (state) {
    case 'up':
      return '✓'
    case 'down':
      return '○'
    case 'starting':
      return '↻'
    case 'unknown':
    default:
      return '?'
  }
}

const getStateLabel = (state: string) => {
  switch (state) {
    case 'up':
      return 'Up'
    case 'down':
      return 'Down'
    case 'starting':
      return 'Starting'
    case 'unknown':
    default:
      return 'Unknown'
  }
}

const formatTime = (timestamp: number) => {
  const now = Math.floor(Date.now() / 1000)
  const diff = now - timestamp
  
  if (diff < 60) {
    return 'just now'
  } else if (diff < 3600) {
    const mins = Math.floor(diff / 60)
    return `${mins} min${mins > 1 ? 's' : ''} ago`
  } else if (diff < 86400) {
    const hours = Math.floor(diff / 3600)
    return `${hours} hour${hours > 1 ? 's' : ''} ago`
  } else {
    const days = Math.floor(diff / 86400)
    return `${days} day${days > 1 ? 's' : ''} ago`
  }
}

onMounted(() => {
  fetchServices()
  // Refresh every 5 seconds
  refreshInterval = window.setInterval(fetchServices, 5000)
})

onUnmounted(() => {
  if (refreshInterval !== null) {
    clearInterval(refreshInterval)
  }
})
</script>

<template>
  <div class="services-overview">
    <div v-if="loading && services.length === 0" class="loading">Loading services...</div>
    <div v-else-if="error" class="error">Error: {{ error }}</div>
    <div v-else-if="services.length === 0" class="empty">No services configured</div>
    <div v-else class="services-grid">
      <div 
        v-for="service in services" 
        :key="service.name"
        class="service-card"
        :class="getStateClass(service.state)"
        @click="navigateToService(service.name)"
      >
        <div class="service-header">
          <div class="service-name">{{ service.name }}</div>
          <div class="service-state">
            <span class="state-icon">{{ getStateIcon(service.state) }}</span>
            <span class="state-label">{{ getStateLabel(service.state) }}</span>
          </div>
        </div>
        <div class="service-footer">
          <span class="last-changed">{{ formatTime(service.last_changed) }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.services-overview {
  width: 100%;
  padding: 20px;
}

.loading,
.error,
.empty {
  padding: 20px;
  text-align: center;
  color: #6b7280;
  font-size: 14px;
}

.error {
  color: #dc2626;
}

.services-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 16px;
}

.service-card {
  background: #ffffff;
  border: 2px solid #e5e7eb;
  border-radius: 8px;
  padding: 16px;
  transition: all 0.2s;
  cursor: pointer;
}

.service-card:hover {
  box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -1px rgba(0, 0, 0, 0.06);
  transform: translateY(-2px);
}

.service-card.state-up {
  border-color: #10b981;
  background: linear-gradient(to bottom, #ffffff 0%, #f0fdf4 100%);
}

.service-card.state-down {
  border-color: #6b7280;
  background: linear-gradient(to bottom, #ffffff 0%, #f9fafb 100%);
}

.service-card.state-starting {
  border-color: #f59e0b;
  background: linear-gradient(to bottom, #ffffff 0%, #fffbeb 100%);
}

.service-card.state-unknown {
  border-color: #9ca3af;
  background: linear-gradient(to bottom, #ffffff 0%, #f3f4f6 100%);
}

.service-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.service-name {
  font-size: 16px;
  font-weight: 600;
  color: #1f2937;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.service-state {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  border-radius: 12px;
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
}

.state-up .service-state {
  background: #d1fae5;
  color: #065f46;
}

.state-down .service-state {
  background: #e5e7eb;
  color: #374151;
}

.state-starting .service-state {
  background: #fef3c7;
  color: #92400e;
}

.state-unknown .service-state {
  background: #f3f4f6;
  color: #6b7280;
}

.state-icon {
  font-size: 16px;
  line-height: 1;
}

.state-starting .state-icon {
  animation: spin 2s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.state-label {
  font-size: 13px;
}

.service-footer {
  display: flex;
  justify-content: flex-end;
  padding-top: 8px;
  border-top: 1px solid #f3f4f6;
}

.last-changed {
  font-size: 12px;
  color: #6b7280;
  font-style: italic;
}
</style>
