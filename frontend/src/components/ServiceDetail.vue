<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRoute } from 'vue-router'
import type { ServiceInfo } from '@/types/api'
import PageHeader from './PageHeader.vue'
import { formatTime } from '@/lib/time'
import { formatServiceName } from '@/lib/utils'

const route = useRoute()
const serviceName = computed(() => route.params.name as string)

const breadcrumbs = computed(() => [
  { label: formatServiceName(serviceName.value) },
  { label: 'Config' }
])

const serviceInfo = ref<ServiceInfo | null>(null)
const serviceConfig = ref<Record<string, any> | null>(null)
const loading = ref(true)
const error = ref<string | null>(null)

const fetchServiceInfo = async () => {
  try {
    error.value = null
    
    // Fetch service state
    const servicesResponse = await fetch('/hibernator-api/services')
    if (!servicesResponse.ok) {
      throw new Error(`HTTP error! status: ${servicesResponse.status}`)
    }
    const services = await servicesResponse.json()
    const service = services.find((s: ServiceInfo) => s.name === serviceName.value)
    
    if (!service) {
      throw new Error(`Service "${serviceName.value}" not found`)
    }
    
    serviceInfo.value = service
    
    // Fetch service config
    const configResponse = await fetch(`/hibernator-api/services/${encodeURIComponent(serviceName.value)}/config`)
    if (!configResponse.ok) {
      throw new Error(`HTTP error! status: ${configResponse.status}`)
    }
    serviceConfig.value = await configResponse.json()
    
    loading.value = false
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to fetch service details'
    console.error('Failed to fetch service details:', e)
    loading.value = false
  }
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

const formatValue = (value: any): string => {
  if (Array.isArray(value)) {
    return value.join(', ')
  } else if (typeof value === 'object' && value !== null) {
    return JSON.stringify(value, null, 2)
  } else if (typeof value === 'boolean') {
    return value ? 'true' : 'false'
  } else {
    return String(value)
  }
}

onMounted(() => {
  fetchServiceInfo()
})
</script>

<template>
  <div class="service-detail">
    <PageHeader :breadcrumbs="breadcrumbs" />

    <div class="content">
      <div v-if="loading" class="loading">Loading service details...</div>
      <div v-else-if="error" class="error">Error: {{ error }}</div>
      <div v-else-if="serviceInfo && serviceConfig">
        <div class="service-header">
          <h2>{{ serviceInfo.name }}</h2>
          <div class="service-state" :class="getStateClass(serviceInfo.state)">
            <span class="state-icon">{{ getStateIcon(serviceInfo.state) }}</span>
            <span class="state-label">{{ getStateLabel(serviceInfo.state) }}</span>
          </div>
        </div>

        <div class="service-meta">
          <div class="meta-item">
            <span class="meta-label">Last State Change:</span>
            <span class="meta-value">{{ formatTime(serviceInfo.last_changed) }}</span>
          </div>
        </div>

        <div class="config-section">
          <h2>Configuration</h2>
          <div class="config-table">
            <div v-for="[key, value] in Object.entries(serviceConfig)" :key="key" class="config-row">
              <div class="config-key">{{ key }}</div>
              <div class="config-value">
                <pre v-if="typeof value === 'object' && value !== null">{{ formatValue(value) }}</pre>
                <span v-else>{{ formatValue(value) }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.service-detail {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  background: #f9fafb;
}

.content {
  flex: 1;
  padding: 24px;
  max-width: 1200px;
  width: 100%;
  margin: 0 auto;
}

.loading,
.error {
  padding: 40px;
  text-align: center;
  font-size: 16px;
}

.loading {
  color: #6b7280;
}

.error {
  color: #dc2626;
}

.service-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 24px;
  padding-bottom: 16px;
  border-bottom: 2px solid #e5e7eb;
}

.service-header h2 {
  font-size: 28px;
  font-weight: 700;
  color: #1f2937;
  margin: 0;
}

.service-state {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  border-radius: 16px;
  font-size: 16px;
  font-weight: 600;
}

.state-up {
  background: #d1fae5;
  color: #065f46;
}

.state-down {
  background: #e5e7eb;
  color: #374151;
}

.state-starting {
  background: #fef3c7;
  color: #92400e;
}

.state-unknown {
  background: #f3f4f6;
  color: #6b7280;
}

.state-icon {
  font-size: 20px;
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

.service-meta {
  background: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  padding: 16px;
  margin-bottom: 24px;
}

.meta-item {
  display: flex;
  gap: 12px;
  font-size: 14px;
}

.meta-label {
  font-weight: 600;
  color: #6b7280;
}

.meta-value {
  color: #1f2937;
}

.config-section {
  background: #ffffff;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  padding: 20px;
}

.config-section h2 {
  font-size: 20px;
  font-weight: 600;
  color: #1f2937;
  margin: 0 0 16px 0;
}

.config-table {
  display: flex;
  flex-direction: column;
  gap: 1px;
  background: #e5e7eb;
  border: 1px solid #e5e7eb;
  border-radius: 6px;
  overflow: hidden;
}

.config-row {
  display: grid;
  grid-template-columns: 250px 1fr;
  background: #ffffff;
}

.config-key {
  padding: 12px 16px;
  font-weight: 600;
  color: #374151;
  background: #f9fafb;
  border-right: 1px solid #e5e7eb;
  font-size: 14px;
  word-break: break-word;
}

.config-value {
  padding: 12px 16px;
  color: #1f2937;
  font-size: 14px;
  font-family: 'Monaco', 'Courier New', monospace;
  word-break: break-word;
}

.config-value pre {
  margin: 0;
  white-space: pre-wrap;
  word-wrap: break-word;
  font-family: inherit;
}

@media (max-width: 768px) {
  .config-row {
    grid-template-columns: 1fr;
  }

  .config-key {
    border-right: none;
    border-bottom: 1px solid #e5e7eb;
  }

  .service-header {
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
  }
}
</style>
