import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import './style.css'
import App from './App.vue'
import AppLayout from './components/AppLayout.vue'
import Dashboard from './components/Dashboard.vue'
import Network from './components/Network.vue'
import ServiceDetail from './components/ServiceDetail.vue'
import ServiceDashboard from './components/ServiceDashboard.vue'
import ServiceStates from './components/ServiceStates.vue'
import Login from './components/Login.vue'
import { isAuthenticated } from './lib/api'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/login',
      component: Login,
      meta: { requiresAuth: false },
    },
    {
      path: '/',
      component: AppLayout,
      meta: { requiresAuth: true },
      children: [
        {
          path: '',
          component: Dashboard,
        },
        {
          path: 'network',
          component: Network,
        },
        {
          path: 'states',
          component: ServiceStates,
        },
        {
          path: 'services/:name/dashboard',
          component: ServiceDashboard,
        },
        {
          path: 'services/:name/config',
          component: ServiceDetail,
        },
        {
          path: 'services/:name/network',
          component: Network,
        },
        {
          path: 'services/:name/states',
          component: ServiceStates,
        },
      ],
    },
  ],
})

// Navigation guard to check authentication
router.beforeEach((to, _from, next) => {
  const requiresAuth = to.matched.some(record => record.meta.requiresAuth !== false)
  
  if (requiresAuth && !isAuthenticated()) {
    next('/login')
  } else if (to.path === '/login' && isAuthenticated()) {
    next('/')
  } else {
    next()
  }
})

const app = createApp(App)
app.use(router)
app.mount('#app')
