<script setup lang="ts">
import { SidebarTrigger } from '@/components/ui/sidebar'
import { Separator } from '@/components/ui/separator'

defineProps<{
  breadcrumbs: Array<{
    label: string
    to?: string
  }>
}>()
</script>

<template>
  <header class="header">
    <div class="header-content">
      <SidebarTrigger class="-ml-1" />
      <Separator orientation="vertical" class="mr-2 h-4" />
      <div class="breadcrumb">
        <template v-for="(crumb, index) in breadcrumbs" :key="index">
          <router-link v-if="crumb.to" :to="crumb.to" class="breadcrumb-link">
            {{ crumb.label }}
          </router-link>
          <span v-else class="breadcrumb-current">{{ crumb.label }}</span>
          <span v-if="index < breadcrumbs.length - 1" class="breadcrumb-separator">/</span>
        </template>
      </div>
    </div>
  </header>
</template>

<style scoped>
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

.breadcrumb {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
}

.breadcrumb-link {
  color: #6b7280;
  text-decoration: none;
  transition: color 0.2s;
}

.breadcrumb-link:hover {
  color: #1f2937;
}

.breadcrumb-separator {
  color: #d1d5db;
  user-select: none;
}

.breadcrumb-current {
  color: #1f2937;
  font-weight: 400;
}
</style>
