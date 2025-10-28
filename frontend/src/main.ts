import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import './style.css'
import App from './App.vue'
import AppLayout from './components/AppLayout.vue'
import Dashboard from './components/Dashboard.vue'
import Logs from './components/Logs.vue'
import ServiceDetail from './components/ServiceDetail.vue'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      component: AppLayout,
      children: [
        {
          path: '',
          component: Dashboard,
        },
        {
          path: 'logs',
          component: Logs,
        },
        {
          path: 'logs/:serviceName',
          component: Logs,
        },
        {
          path: 'services/:name',
          component: ServiceDetail,
        },
      ],
    },
  ],
})

const app = createApp(App)
app.use(router)
app.mount('#app')
