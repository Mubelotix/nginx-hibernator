<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import LogsTable from './LogsTable.vue'
import PageHeader from './PageHeader.vue'
import { formatServiceName } from '@/lib/utils'

const route = useRoute()

const serviceName = computed(() => {
  return route.params.name as string | undefined
})

const breadcrumbs = computed(() => {
  if (serviceName.value) {
    return [
      { label: formatServiceName(serviceName.value), to: `/services/${serviceName.value}/config` },
      { label: 'Network' }
    ]
  } else {
    return [{ label: 'Network Logs' }]
  }
})
</script>

<template>
  <div class="network-page">
    <PageHeader :breadcrumbs="breadcrumbs" />
    <div class="content">
      <LogsTable :service-name="serviceName" />
    </div>
  </div>
</template>

<style scoped>
.network-page {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  background: #f9fafb;
}

.content {
  flex: 1;
}
</style>
