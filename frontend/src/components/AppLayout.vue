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
import { LayoutDashboard, Activity, Server, ChevronRight, Cctv } from 'lucide-vue-next'
import { formatServiceName } from '@/lib/utils'

const route = useRoute()

const menuItems = [
  {
    title: 'Dashboard',
    icon: LayoutDashboard,
    path: '/',
  },
  {
    title: 'All Network Logs',
    icon: Cctv,
    path: '/network',
  },
  {
    title: 'All State Logs',
    icon: Activity,
    path: '/states',
  },
]

const services = ref<ServiceInfo[]>([])
const expandedServices = ref<Set<string>>(new Set())
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

const toggleService = (serviceName: string) => {
  if (expandedServices.value.has(serviceName)) {
    expandedServices.value.delete(serviceName)
  } else {
    expandedServices.value.add(serviceName)
  }
}

const isServiceExpanded = (serviceName: string) => {
  return expandedServices.value.has(serviceName)
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

        <SidebarGroup v-for="service in services" :key="service.name">
          <SidebarGroupLabel @click="toggleService(service.name)" class="cursor-pointer">
            <div class="flex items-center justify-between w-full">
              <div class="flex items-center gap-2">
                <Server :size="16" />
                <span>{{ formatServiceName(service.name) }}</span>
              </div>
              <ChevronRight 
                :size="16" 
                :class="{ 'rotate-90': isServiceExpanded(service.name) }"
                class="transition-transform"
              />
            </div>
          </SidebarGroupLabel>
          <SidebarGroupContent v-if="isServiceExpanded(service.name)">
            <SidebarMenuSub>
              <SidebarMenuSubItem>
                <SidebarMenuSubButton
                  as-child
                  :is-active="route.path === `/services/${service.name}/dashboard`"
                >
                  <router-link :to="`/services/${service.name}/dashboard`">
                    <span>Dashboard</span>
                  </router-link>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
              <SidebarMenuSubItem>
                <SidebarMenuSubButton
                  as-child
                  :is-active="route.path === `/services/${service.name}/config`"
                >
                  <router-link :to="`/services/${service.name}/config`">
                    <span>Config</span>
                  </router-link>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
              <SidebarMenuSubItem>
                <SidebarMenuSubButton
                  as-child
                  :is-active="route.path === `/services/${service.name}/network`"
                >
                  <router-link :to="`/services/${service.name}/network`">
                    <span>Network</span>
                  </router-link>
                </SidebarMenuSubButton>
              </SidebarMenuSubItem>
              <SidebarMenuSubItem>
                <SidebarMenuSubButton
                  as-child
                  :is-active="route.path === `/services/${service.name}/states`"
                >
                  <router-link :to="`/services/${service.name}/states`">
                    <span>States</span>
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
