<script setup lang="ts">
import { computed } from 'vue'
import { ChevronDown, X } from '@lucide/vue'
import { dateDaysAgo } from '@/services/format'
import { useTaskStore } from '@/stores/tasks'
import type { SortKey } from '@/types'
import { PRIORITIES } from '@/types/priority'

const tasks = useTaskStore()
const sortModel = computed<SortKey>({
  get: () => tasks.sort,
  set: (value) => void tasks.setSort(value),
})
const presets = [
  { label: '今天', days: 0 },
  { label: '昨天', days: 1 },
]
function applyPreset(days: number) {
  const day = dateDaysAgo(days)
  void tasks.setDateFilter(tasks.dateFilter === day ? null : day)
}
function onDate(event: Event, edge: 'from' | 'to') {
  const value = (event.target as HTMLInputElement).value || null
  void tasks.setDateRange(
    edge === 'from' ? value : tasks.dateFrom,
    edge === 'to' ? value : tasks.dateTo
  )
}
</script>

<template>
  <fieldset
    class="flex flex-wrap items-center gap-1.5"
    :disabled="tasks.batchBusy"
  >
    <template v-if="tasks.view !== 'today'">
      <div
        class="flex items-center gap-0.5 rounded-md border border-line bg-panel p-[2px]"
      >
        <button
          v-for="preset in presets"
          :key="preset.days"
          type="button"
          class="filter-chip"
          :class="{
            'is-active': tasks.dateFilter === dateDaysAgo(preset.days),
          }"
          @click="applyPreset(preset.days)"
        >
          {{ preset.label }}
        </button>
      </div>
      <label class="flex items-center gap-1 text-[11px] text-ink-3"
        >记录日期
        <input
          type="date"
          class="date-input input h-7 w-[123px] px-1 text-[11.5px]"
          aria-label="记录开始日期"
          :value="tasks.dateFrom ?? ''"
          @change="onDate($event, 'from')"
        />
      </label>
      <span class="text-[11px] text-ink-3">至</span>
      <input
        type="date"
        class="date-input input h-7 w-[123px] px-1 text-[11.5px]"
        aria-label="记录结束日期（含当天）"
        title="结束日期包含当天"
        :value="tasks.dateTo ?? ''"
        @change="onDate($event, 'to')"
      />
      <button
        v-if="tasks.dateFrom || tasks.dateTo"
        type="button"
        class="grid h-7 w-6 place-items-center rounded text-ink-3 hover:bg-hover"
        title="清除日期筛选"
        @click="tasks.setDateRange(null, null)"
      >
        <X :size="13" />
      </button>
    </template>
    <span v-else class="rounded-md bg-panel-2 px-2 py-1 text-[11px] text-ink-2"
      >记录日期：今天（搜索仍限今天）</span
    >
    <div
      class="flex items-center gap-0.5 rounded-md border border-line bg-panel p-[2px]"
    >
      <button
        type="button"
        class="filter-chip"
        :class="{ 'is-active': tasks.priorityFilter === null }"
        @click="tasks.setPriorityFilter(null)"
      >
        全部优先级
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
        class="input h-7 w-[106px] appearance-none pl-2 pr-6 text-[11.5px]"
        aria-label="排序方式"
      >
        <option value="priority">按优先级</option>
        <option value="recorded">按记录时间</option>
        <option value="created">按创建时间</option>
        <option value="updated">按更新时间</option>
        <option value="manual">手动排序</option>
      </select>
      <ChevronDown
        :size="12"
        class="pointer-events-none absolute right-1.5 top-1/2 -translate-y-1/2 text-ink-3"
      />
    </div>
  </fieldset>
  <p
    v-if="tasks.dateError && tasks.view !== 'today'"
    role="alert"
    class="mt-1 text-[11.5px] text-p0"
  >
    {{ tasks.dateError }}
  </p>
  <p v-if="tasks.sort === 'manual'" class="mt-1 text-[11px] text-ink-3">
    {{
      tasks.reorderReason ||
      '拖动事项左侧手柄调整顺序；更多菜单可移动到相邻页，顺序会自动保存。'
    }}
  </p>
</template>
