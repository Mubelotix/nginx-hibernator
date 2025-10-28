<script setup lang="ts">
import { ref, onMounted, watch, computed } from 'vue'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import type { ServiceMetrics } from '@/types/api'

const props = defineProps<{
  serviceName: string
}>()

const metrics = ref<ServiceMetrics | null>(null)
const loading = ref(true)
const error = ref<string | null>(null)
const selectedPeriod = ref<number>(86400) // Default 24 hours

const periods = [
  { label: '24 hours', value: 86400 },
  { label: '7 days', value: 604800 },
  { label: '30 days', value: 2592000 },
]

const fetchMetrics = async () => {
  try {
    loading.value = true
    error.value = null
    
    const response = await fetch(
      `/hibernator-api/services/${encodeURIComponent(props.serviceName)}/metrics?seconds=${selectedPeriod.value}`
    )
    
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`)
    }
    
    metrics.value = await response.json()
    loading.value = false
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to fetch metrics'
    console.error('Failed to fetch metrics:', e)
    loading.value = false
  }
}

const histogramData = computed(() => {
  if (!metrics.value) return []
  
  const labels = ['0-1s', '1-5s', '5-10s', '10-30s', '30s+']
  return metrics.value.start_times_histogram.map((count, index) => ({
    range: labels[index],
    count,
  }))
})

const maxCount = computed(() => {
  if (!histogramData.value.length) return 0
  return Math.max(...histogramData.value.map(d => d.count))
})

const formatDuration = (ms: number | null) => {
  if (ms === null) return 'N/A'
  if (ms < 1000) return `${ms}ms`
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`
  return `${(ms / 60000).toFixed(1)}m`
}

const uptimeColor = computed(() => {
  if (!metrics.value) return 'text-gray-600'
  const uptime = metrics.value.uptime_percentage
  if (uptime >= 95) return 'text-green-600'
  if (uptime >= 80) return 'text-yellow-600'
  return 'text-red-600'
})

watch(() => props.serviceName, fetchMetrics, { immediate: false })
watch(selectedPeriod, fetchMetrics)

onMounted(() => {
  fetchMetrics()
})
</script>

<template>
  <div class="service-metrics">
    <div class="period-selector">
      <Button
        v-for="period in periods"
        :key="period.value"
        :variant="selectedPeriod === period.value ? 'default' : 'outline'"
        size="sm"
        @click="selectedPeriod = period.value"
      >
        {{ period.label }}
      </Button>
    </div>

    <div v-if="loading" class="loading">Loading metrics...</div>
    <div v-else-if="error" class="error">Error: {{ error }}</div>
    
    <div v-else-if="metrics" class="metrics-grid">
      <!-- Uptime Card -->
      <Card>
        <CardHeader>
          <CardTitle>Uptime</CardTitle>
          <CardDescription>Service availability over selected period</CardDescription>
        </CardHeader>
        <CardContent>
          <div class="metric-value" :class="uptimeColor">
            {{ metrics.uptime_percentage.toFixed(2) }}%
          </div>
        </CardContent>
      </Card>

      <!-- Hibernations Card -->
      <Card>
        <CardHeader>
          <CardTitle>Total Hibernations</CardTitle>
          <CardDescription>Sleep/wake cycles in selected period</CardDescription>
        </CardHeader>
        <CardContent>
          <div class="metric-value text-blue-600">
            {{ metrics.total_hibernations }}
          </div>
        </CardContent>
      </Card>

      <!-- Start Duration Estimate Card -->
      <Card>
        <CardHeader>
          <CardTitle>Estimated Start Time</CardTitle>
          <CardDescription>Expected duration for service to start</CardDescription>
        </CardHeader>
        <CardContent>
          <div class="metric-value text-purple-600">
            {{ formatDuration(metrics.start_duration_estimate_ms) }}
          </div>
          <div v-if="metrics.start_duration_estimate_ms" class="metric-note">
            Based on historical data
          </div>
        </CardContent>
      </Card>

      <!-- Start Times Histogram -->
      <Card class="histogram-card">
        <CardHeader>
          <CardTitle>Start Time Distribution</CardTitle>
          <CardDescription>How long it takes for the service to start</CardDescription>
        </CardHeader>
        <CardContent>
          <div v-if="histogramData.some(d => d.count > 0)" class="histogram">
            <div v-for="item in histogramData" :key="item.range" class="bar-container">
              <div class="bar-label">{{ item.range }}</div>
              <div class="bar-wrapper">
                <div 
                  class="bar"
                  :style="{ width: maxCount > 0 ? `${(item.count / maxCount) * 100}%` : '0%' }"
                >
                  <span class="bar-count">{{ item.count }}</span>
                </div>
              </div>
            </div>
          </div>
          <div v-else class="no-data">
            No start time data available for this period
          </div>
        </CardContent>
      </Card>
    </div>
  </div>
</template>

<style scoped>
.service-metrics {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.period-selector {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
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

.metrics-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  gap: 20px;
}

.histogram-card {
  grid-column: 1 / -1;
}

.metric-value {
  font-size: 48px;
  font-weight: 700;
  line-height: 1;
  margin-bottom: 8px;
}

.metric-note {
  font-size: 12px;
  color: #6b7280;
  margin-top: 4px;
}

.histogram {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 20px 0;
}

.bar-container {
  display: flex;
  align-items: center;
  gap: 12px;
}

.bar-label {
  min-width: 60px;
  font-size: 14px;
  font-weight: 500;
  color: #374151;
}

.bar-wrapper {
  flex: 1;
  height: 32px;
  background: #f3f4f6;
  border-radius: 4px;
  overflow: hidden;
}

.bar {
  height: 100%;
  background: linear-gradient(90deg, #3b82f6 0%, #60a5fa 100%);
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding-right: 8px;
  transition: width 0.3s ease;
  min-width: fit-content;
}

.bar-count {
  font-size: 12px;
  font-weight: 600;
  color: white;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.2);
}

.no-data {
  padding: 60px 20px;
  text-align: center;
  color: #6b7280;
  font-size: 14px;
}

@media (max-width: 768px) {
  .metrics-grid {
    grid-template-columns: 1fr;
  }
  
  .metric-value {
    font-size: 36px;
  }
  
  .bar-label {
    min-width: 50px;
    font-size: 12px;
  }
}
</style>
