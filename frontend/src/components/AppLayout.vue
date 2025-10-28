<script setup lang="ts">
import { useRoute } from 'vue-router'
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarInset,
} from '@/components/ui/sidebar'
import { LayoutDashboard, FileText } from 'lucide-vue-next'

const route = useRoute()

const menuItems = [
  {
    title: 'Dashboard',
    icon: LayoutDashboard,
    path: '/',
  },
  {
    title: 'Logs',
    icon: FileText,
    path: '/logs',
  },
]
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
      </SidebarContent>
    </Sidebar>
    <SidebarInset>
      <router-view />
    </SidebarInset>
  </SidebarProvider>
</template>

<style scoped>
</style>
