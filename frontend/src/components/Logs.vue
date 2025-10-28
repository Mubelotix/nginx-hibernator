<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import LogsTable from './LogsTable.vue'
import { SidebarTrigger } from '@/components/ui/sidebar'
import { Separator } from '@/components/ui/separator'

const route = useRoute()

const serviceName = computed(() => {
  return route.params.serviceName as string | undefined
})

const pageTitle = computed(() => {
  return serviceName.value ? `Logs - ${serviceName.value}` : 'Logs'
})
</script>

<template>
  <div class="logs-page">
    <header class="header">
      <div class="header-content">
        <SidebarTrigger class="-ml-1" />
        <Separator orientation="vertical" class="mr-2 h-4" />
        <h1 class="header-title">{{ pageTitle }}</h1>
      </div>
    </header>
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

.header {
  position: sticky;
  top: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 2px;
  border-bottom: 1px solid #e5e7eb;
  background: #ffffff;
  padding: 0 16px;
  height: 57px;
  flex-shrink: 0;
}

.header-content {
  display: flex;
  align-items: center;
  gap: 8px;
}

.header-title {
  font-size: 16px;
  font-weight: 600;
  color: #1f2937;
  margin: 0;
}

.content {
  flex: 1;
}
</style>
