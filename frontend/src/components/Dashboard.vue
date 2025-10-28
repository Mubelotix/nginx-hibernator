<script setup lang="ts">
import ServicesOverview from './ServicesOverview.vue'
import LogsTable from './LogsTable.vue'
</script>

<template>
  <div class="dashboard">
    <ServicesOverview />
    <LogsTable />
  </div>
</template>

<style scoped>
.dashboard {
  width: 100%;
  min-height: 100vh;
  background: #f9fafb;
}
</style>
