<script setup lang="ts">
import { computed, watch } from 'vue'
import { useRoute } from 'vue-router'

import TaskList from '@/components/TaskList.vue'
import { useTaskStore } from '@/stores/tasks'
import type { ViewKey } from '@/types'

const route = useRoute()
const tasks = useTaskStore()

const view = computed<ViewKey>(() =>
  typeof route.meta.view === 'string' ? (route.meta.view as ViewKey) : 'all'
)

watch(
  view,
  (next) => {
    void tasks.syncView(next)
  },
  { immediate: true }
)
</script>

<template>
  <TaskList />
</template>
