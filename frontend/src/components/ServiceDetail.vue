<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useRoute } from 'vue-router'
import PageHeader from './PageHeader.vue'
import { formatServiceName } from '@/lib/utils'
import { apiFetch } from '@/lib/api'

const route = useRoute()
const serviceName = computed(() => route.params.name as string)

const breadcrumbs = computed(() => [
  { label: formatServiceName(serviceName.value) },
  { label: 'Config' }
])

const serviceConfig = ref<Record<string, any> | null>(null)
const loading = ref(true)
const error = ref<string | null>(null)

const fetchServiceInfo = async () => {
  try {
    error.value = null
    
    // Fetch service config
    const configResponse = await apiFetch(`/hibernator-api/services/${encodeURIComponent(serviceName.value)}/config`)
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
      <div v-if="loading" class="loading">Loading service configuration...</div>
      <div v-else-if="error" class="error">Error: {{ error }}</div>
      <div v-else-if="serviceConfig" class="config-table">
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
}
</style>
