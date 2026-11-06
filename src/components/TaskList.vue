<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { AlertTriangle, CalendarDays, ChevronDown, X } from '@lucide/vue'

import EmptyState from '@/components/EmptyState.vue'
import TaskItem from '@/components/TaskItem.vue'
import TaskQuickAdd from '@/components/TaskQuickAdd.vue'
import { dateDaysAgo } from '@/services/format'
import { useTaskStore } from '@/stores/tasks'
import type { SortKey } from '@/types'
import { PRIORITIES, VIEW_HINTS, VIEW_LABELS } from '@/types/priority'

const tasks = useTaskStore()
const route = useRoute()

const view = computed(() => (typeof route.meta.view === 'string' ? route.meta.view : 'all'))

const title = computed(() =>
  tasks.isSearching ? '搜索结果' : VIEW_LABELS[view.value] ?? '事项列表'
)

const hint = computed(() => {
  if (tasks.isSearching) {
    return `关键词「${tasks.search.trim()}」 · 同时匹配标题、描述、负责部门与全部历史进展`
  }
  return VIEW_HINTS[view.value] ?? ''
})

const canQuickAdd = computed(() => !['trash', 'hidden', 'done'].includes(view.value))

/** 「今日事项」本身就是日期视图，不再重复提供日期筛选 */
const showDateFilter = computed(() => view.value !== 'today')

const sortModel = computed<SortKey>({
  get: () => tasks.sort,
  set: (value) => void tasks.setSort(value),
})

const countLabel = computed(() => `${tasks.tasks.length} 条`)

function onDateChange(event: Event) {
  const value = (event.target as HTMLInputElement).value
  void tasks.setDateFilter(value || null)
}

/** 常用日期快捷项：今天 / 昨天 */
const datePresets = [
  { label: '今天', days: 0 },
  { label: '昨天', days: 1 },
]

function isPresetActive(days: number): boolean {
  return tasks.dateFilter === dateDaysAgo(days)
}

function applyPreset(days: number) {
  // 再次点击同一快捷项则取消筛选
  void tasks.setDateFilter(isPresetActive(days) ? null : dateDaysAgo(days))
}
</script>

<template>
  <section class="flex min-h-0 flex-1 flex-col">
    <!-- 列表工具条 -->
    <div class="flex flex-wrap items-center gap-x-2 gap-y-1.5 px-3.5 pb-1.5 pt-3">
      <h2 class="shrink-0 whitespace-nowrap text-[13px] font-semibold text-ink">{{ title }}</h2>
      <span
        class="shrink-0 rounded-full bg-panel-2 px-1.5 py-[1px] text-[10.5px] tabular-nums text-ink-3"
      >
        {{ countLabel }}
      </span>
      <span class="hidden min-w-0 flex-1 truncate text-[11.5px] text-ink-3 xl:inline">{{ hint }}</span>

      <div class="ml-auto flex shrink-0 items-center gap-1.5">
        <div v-if="showDateFilter" class="flex items-center gap-1">
          <div class="flex items-center gap-0.5 rounded-md border border-line bg-panel p-[2px]">
            <button
              v-for="preset in datePresets"
              :key="preset.label"
              type="button"
              class="filter-chip"
              :class="{ 'is-active': isPresetActive(preset.days) }"
              :title="`只看${preset.label}记录的事项`"
              @click="applyPreset(preset.days)"
            >
              {{ preset.label }}
            </button>
          </div>
          <div class="relative">
            <CalendarDays
              :size="12"
              class="pointer-events-none absolute left-1.5 top-1/2 -translate-y-1/2 text-ink-3"
            />
            <input
              type="date"
              class="date-input input h-7 w-[124px] pl-6 pr-1 text-[11.5px]"
              :value="tasks.dateFilter ?? ''"
              title="按记录日期筛选事项"
              @change="onDateChange"
            />
          </div>
          <button
            v-if="tasks.dateFilter"
            type="button"
            class="grid h-7 w-7 place-items-center rounded border border-line bg-panel text-ink-3 hover:bg-hover hover:text-ink"
            title="清除日期筛选"
            @click="tasks.setDateFilter(null)"
          >
            <X :size="13" />
          </button>
        </div>

        <div class="flex items-center gap-0.5 rounded-md border border-line bg-panel p-[2px]">
          <button
            type="button"
            class="filter-chip"
            :class="{ 'is-active': tasks.priorityFilter === null }"
            @click="tasks.setPriorityFilter(null)"
          >
            全部
          </button>
          <button
            v-for="option in PRIORITIES"
            :key="option.value"
            type="button"
            class="filter-chip"
            :class="{ 'is-active': tasks.priorityFilter === option.value }"
            @click="tasks.setPriorityFilter(option.value)"
          >
            {{ option.code }}
          </button>
        </div>

        <div class="relative">
          <select
            v-model="sortModel"
            class="input h-7 w-[106px] cursor-default appearance-none pl-2 pr-6 text-[11.5px]"
            title="排序方式"
          >
            <option value="priority">按优先级</option>
            <option value="recorded">按记录时间</option>
            <option value="created">按创建时间</option>
            <option value="updated">按更新时间</option>
          </select>
          <ChevronDown
            :size="12"
            class="pointer-events-none absolute right-1.5 top-1/2 -translate-y-1/2 text-ink-3"
          />
        </div>
      </div>
    </div>

    <div v-if="tasks.error" class="mx-3.5 mb-1.5 flex items-start gap-2 rounded-md bg-p0-soft px-2.5 py-1.5">
      <AlertTriangle :size="13" class="mt-[2px] shrink-0 text-p0" />
      <p class="flex-1 text-[11.5px] leading-relaxed text-p0">{{ tasks.error }}</p>
      <button type="button" class="text-[11px] font-medium text-p0 underline" @click="tasks.refresh()">
        重试
      </button>
    </div>

    <TaskQuickAdd v-if="canQuickAdd" />

    <div class="min-h-0 flex-1 overflow-y-auto px-3 pb-4">
      <div
        v-if="tasks.loading && tasks.tasks.length === 0"
        class="flex flex-col gap-2 py-3"
        aria-busy="true"
      >
        <div v-for="index in 3" :key="index" class="h-[62px] animate-pulse rounded-lg bg-panel-2" />
        <p class="text-center text-[11.5px] text-ink-3">正在读取本地数据…</p>
      </div>

      <TaskItem v-for="task in tasks.tasks" :key="task.id" :task="task" />

      <EmptyState
        v-if="!tasks.loading && tasks.tasks.length === 0"
        :view="view"
        :searching="tasks.isSearching"
      />
    </div>
  </section>
</template>
