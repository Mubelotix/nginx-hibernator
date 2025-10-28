import { createApp } from 'vue'
import { createRouter, createWebHistory } from 'vue-router'
import './style.css'
import App from './App.vue'
import RequestsTable from './components/RequestsTable.vue'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      component: RequestsTable,
    },
  ],
})

const app = createApp(App)
app.use(router)
app.mount('#app')
