import { createRouter, createWebHashHistory } from 'vue-router'

import TaskListView from '@/views/TaskListView.vue'
import SettingsView from '@/views/SettingsView.vue'

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'all', component: TaskListView, meta: { view: 'all' } },
    { path: '/today', name: 'today', component: TaskListView, meta: { view: 'today' } },
    { path: '/open', name: 'open', component: TaskListView, meta: { view: 'open' } },
    { path: '/focus', name: 'focus', component: TaskListView, meta: { view: 'focus' } },
    { path: '/done', name: 'done', component: TaskListView, meta: { view: 'done' } },
    { path: '/hidden', name: 'hidden', component: TaskListView, meta: { view: 'hidden' } },
    { path: '/trash', name: 'trash', component: TaskListView, meta: { view: 'trash' } },
    { path: '/settings', name: 'settings', component: SettingsView },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})

export default router
