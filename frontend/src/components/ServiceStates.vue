<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import StateHistoryTable from './StateHistoryTable.vue'
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
      { label: 'States' }
    ]
  } else {
    return [{ label: 'State Logs' }]
  }
})
</script>

<template>
  <div class="states-page">
    <PageHeader :breadcrumbs="breadcrumbs" />
    <div class="content">
      <StateHistoryTable :service-name="serviceName" />
    </div>
  </div>
</template>

<style scoped>
.states-page {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
  background: #f9fafb;
}

.content {
  flex: 1;
}
</style>
