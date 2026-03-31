<script setup lang="ts">
import ServicesOverview from './ServicesOverview.vue'
import { SidebarTrigger } from '@/components/ui/sidebar'
import { Separator } from '@/components/ui/separator'
</script>

<template>
  <div class="dashboard">
    <header class="header">
      <div class="header-content">
        <SidebarTrigger class="-ml-1" />
        <Separator orientation="vertical" class="mr-2 h-4" />
        <h1 class="header-title">Dashboard</h1>
      </div>
    </header>
    <div class="content">
      <ServicesOverview />
    </div>
  </div>
</template>

<style scoped>
.dashboard {
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
