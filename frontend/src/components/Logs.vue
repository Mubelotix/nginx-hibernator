<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import LogsTable from './LogsTable.vue'
import PageHeader from './PageHeader.vue'
import { formatServiceName } from '@/lib/utils'

const route = useRoute()

const serviceName = computed(() => {
  return route.params.serviceName as string | undefined
})

const breadcrumbs = computed(() => {
  if (serviceName.value) {
    return [
      { label: formatServiceName(serviceName.value), to: `/services/${serviceName.value}` },
      { label: 'Logs' }
    ]
  } else {
    return [{ label: 'Logs' }]
  }
})
</script>

<template>
  <div class="logs-page">
    <PageHeader :breadcrumbs="breadcrumbs" />
    <div class="content">
      <LogsTable :service-name="serviceName" />
    </div>
  </div>
</template>

<style scoped>
.logs-page {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  background: #f9fafb;
}

.content {
  flex: 1;
}
</style>
