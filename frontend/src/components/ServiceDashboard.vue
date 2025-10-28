<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import PageHeader from './PageHeader.vue'
import ServiceMetrics from './ServiceMetrics.vue'
import { formatServiceName } from '@/lib/utils'

const route = useRoute()
const serviceName = computed(() => route.params.name as string)

const breadcrumbs = computed(() => [
  { label: formatServiceName(serviceName.value) },
  { label: 'Dashboard' }
])
</script>

<template>
  <div class="service-dashboard">
    <PageHeader :breadcrumbs="breadcrumbs" />

    <div class="content">
      <ServiceMetrics :service-name="serviceName" />
    </div>
  </div>
</template>

<style scoped>
.service-dashboard {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  background: #f9fafb;
}

.content {
  flex: 1;
  padding: 24px;
  max-width: 1400px;
  width: 100%;
  margin: 0 auto;
}
</style>
