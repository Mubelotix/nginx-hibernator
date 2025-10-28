<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useRoute } from 'vue-router'
import type { ServiceInfo } from '@/types/api'
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubItem,
  SidebarMenuSubButton,
  SidebarProvider,
  SidebarInset,
} from '@/components/ui/sidebar'
import { LayoutDashboard, FileText, Server, ChevronRight } from 'lucide-vue-next'

const route = useRoute()

const menuItems = [
  {
    title: 'Dashboard',
    icon: LayoutDashboard,
    path: '/',
  },
]

const services = ref<ServiceInfo[]>([])
const servicesExpanded = ref(true)
const logsExpanded = ref(true)
let refreshInterval: number | null = null

const fetchServices = async () => {
  try {
    const response = await fetch('/hibernator-api/services')
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`)
    }
    const data = await response.json()
    services.value = data
  } catch (e) {
    console.error('Failed to fetch services:', e)
  }
}

const toggleServices = () => {
  servicesExpanded.value = !servicesExpanded.value
}

const toggleLogs = () => {
  logsExpanded.value = !logsExpanded.value
}

onMounted(() => {
  fetchServices()
  // Refresh services every 5 seconds
  refreshInterval = window.setInterval(fetchServices, 5000)
})

onUnmounted(() => {
  if (refreshInterval !== null) {
    clearInterval(refreshInterval)
  }
})
</script>

<template>
  <SidebarProvider>
    <Sidebar>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
              <SidebarMenuItem v-for="item in menuItems" :key="item.path">
                <SidebarMenuButton
                  as-child
                  :is-active="route.path === item.path"
                >
                  <router-link :to="item.path">
                    <component :is="item.icon" />
                    <span>{{ item.title }}</span>
                  </router-link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>

        <SidebarGroup>
          <SidebarGroupLabel @click="toggleLogs" class="cursor-pointer">
            <div class="flex items-center justify-between w-full">
              <div class="flex items-center gap-2">
                <FileText :size="16" />
                <span>Logs</span>
              </div>
              <ChevronRight 
                :size="16" 
                :class="{ 'rotate-90': logsExpanded }"
                class="transition-transform"
              />
            </div>
          </SidebarGroupLabel>
          <SidebarGroupContent v-if="logsExpanded">
            <SidebarMenuSub>
              <SidebarMenuSubItem>
                <SidebarMenuSubButton
                  as-child
                  :is-active="route.path === '/logs'"
                >
                  <router-link to="/logs">
                    <span>All Services</span>
                  </router-link>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
              <SidebarMenuSubItem v-for="service in services" :key="`log-${service.name}`">
                <SidebarMenuSubButton
                  as-child
                  :is-active="route.path === `/logs/${service.name}`"
                >
                  <router-link :to="`/logs/${service.name}`">
                    <span>{{ service.name }}</span>
                  </router-link>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
            </SidebarMenuSub>
          </SidebarGroupContent>
        </SidebarGroup>

        <SidebarGroup>
          <SidebarGroupLabel @click="toggleServices" class="cursor-pointer">
            <div class="flex items-center justify-between w-full">
              <div class="flex items-center gap-2">
                <Server :size="16" />
                <span>Services</span>
              </div>
              <ChevronRight 
                :size="16" 
                :class="{ 'rotate-90': servicesExpanded }"
                class="transition-transform"
              />
            </div>
          </SidebarGroupLabel>
          <SidebarGroupContent v-if="servicesExpanded">
            <SidebarMenuSub>
              <SidebarMenuSubItem v-for="service in services" :key="service.name">
                <SidebarMenuSubButton
                  as-child
                  :is-active="route.path === `/services/${service.name}`"
                >
                  <router-link :to="`/services/${service.name}`">
                    <span>{{ service.name }}</span>
                  </router-link>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
            </SidebarMenuSub>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
    </Sidebar>
    <SidebarInset>
      <router-view />
    </SidebarInset>
  </SidebarProvider>
</template>

<style scoped>
.cursor-pointer {
  cursor: pointer;
}

.rotate-90 {
  transform: rotate(90deg);
}

.transition-transform {
  transition: transform 0.2s ease-in-out;
}
</style>
