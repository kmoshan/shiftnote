<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { CalendarClock, Plus, Search, Settings, X } from '@lucide/vue'

import { useNow } from '@/composables/useNow'
import { useTaskStore } from '@/stores/tasks'
import { useUiStore } from '@/stores/ui'
import { VIEW_LABELS } from '@/types/priority'

const tasks = useTaskStore()
const ui = useUiStore()
const route = useRoute()
const router = useRouter()
const now = useNow(15000)

const searchInput = ref<HTMLInputElement | null>(null)
const searchText = ref('')
let debounceTimer: number | undefined

const weekdayNames = ['周日', '周一', '周二', '周三', '周四', '周五', '周六']

const dateLabel = computed(() => {
  const d = new Date(now.value)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(
    d.getDate()
  ).padStart(2, '0')} ${weekdayNames[d.getDay()]}`
})

const clockLabel = computed(() => {
  const d = new Date(now.value)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
})

const title = computed(() => {
  if (tasks.isSearching) return '搜索结果'
  const view = route.meta.view
  return typeof view === 'string' ? VIEW_LABELS[view] ?? 'ShiftNote' : 'ShiftNote'
})

function applySearch(value: string) {
  searchText.value = value
  window.clearTimeout(debounceTimer)
  debounceTimer = window.setTimeout(() => {
    void tasks.setSearch(value)
  }, 180)
}

function clearSearch() {
  applySearch('')
  searchInput.value?.focus()
}

function focusSearch() {
  searchInput.value?.focus()
  searchInput.value?.select()
}

onMounted(() => {
  window.addEventListener('shiftnote:focus-search', focusSearch)
  searchText.value = tasks.search
})

onUnmounted(() => {
  window.removeEventListener('shiftnote:focus-search', focusSearch)
  window.clearTimeout(debounceTimer)
})
</script>

<template>
  <header class="flex h-13 shrink-0 items-center gap-3 border-b border-line bg-panel px-3.5 py-2">
    <div class="min-w-0">
      <h1 class="truncate text-[13.5px] font-semibold text-ink">{{ title }}</h1>
      <p v-if="tasks.isSearching" class="truncate text-[10.5px] text-ink-3">
        匹配标题、描述、负责部门与全部历史进展（含已完成 / 已隐藏）
      </p>
    </div>

    <div class="ml-auto flex items-center gap-2">
      <div class="hidden items-center gap-1.5 text-[11.5px] text-ink-3 lg:flex">
        <CalendarClock :size="13" />
        <span class="tabular-nums">{{ dateLabel }}</span>
        <span class="font-medium tabular-nums text-ink-2">{{ clockLabel }}</span>
      </div>

      <div class="relative">
        <Search
          :size="14"
          class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-ink-3"
        />
        <input
          ref="searchInput"
          :value="searchText"
          type="text"
          class="input h-8 w-[232px] pl-8 pr-7 text-[12.5px]"
          placeholder="搜索事项 / 进展 / 部门"
          title="Ctrl+F 聚焦搜索"
          @input="applySearch(($event.target as HTMLInputElement).value)"
          @keydown.esc.prevent="clearSearch"
        />
        <button
          v-if="searchText"
          type="button"
          class="absolute right-1.5 top-1/2 grid h-5 w-5 -translate-y-1/2 place-items-center rounded text-ink-3 hover:bg-hover hover:text-ink"
          title="清空搜索"
          @click="clearSearch"
        >
          <X :size="13" />
        </button>
      </div>

      <button class="btn btn-primary h-8" type="button" title="新建事项 (Ctrl+N)" @click="ui.openCreate()">
        <Plus :size="14" :stroke-width="2.4" />
        新建事项
      </button>

      <button
        class="btn btn-ghost h-8 w-8 px-0"
        type="button"
        title="设置"
        @click="router.push('/settings')"
      >
        <Settings :size="15" />
      </button>
    </div>
  </header>
</template>
