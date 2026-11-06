<script setup lang="ts">
import { computed } from 'vue'
import type { Component } from 'vue'
import { useRoute } from 'vue-router'
import {
  CalendarDays,
  Archive,
  ClipboardList,
  CheckCircle2,
  EyeOff,
  Flame,
  Inbox,
  ListTodo,
  Moon,
  Settings,
  Sun,
  Trash2,
} from '@lucide/vue'

import { useTaskStore } from '@/stores/tasks'
import { useUiStore } from '@/stores/ui'

const tasks = useTaskStore()
const ui = useUiStore()
const route = useRoute()

interface NavEntry {
  to: string
  label: string
  icon: Component
  count: number
  emphasis?: boolean
}

const primaryItems = computed<NavEntry[]>(() => [
  { to: '/', label: '全部事项', icon: Inbox, count: tasks.counts.all },
  {
    to: '/today',
    label: '今日事项',
    icon: CalendarDays,
    count: tasks.counts.today,
  },
  {
    to: '/open',
    label: '待处理事项',
    icon: ListTodo,
    count: tasks.counts.open,
  },
  {
    to: '/focus',
    label: '重点事项',
    icon: Flame,
    count: tasks.counts.focus,
    emphasis: true,
  },
])

const secondaryItems = computed<NavEntry[]>(() => [
  {
    to: '/done',
    label: '已完成事项',
    icon: CheckCircle2,
    count: tasks.counts.done,
  },
  {
    to: '/hidden',
    label: '已隐藏事项',
    icon: EyeOff,
    count: tasks.counts.hidden,
  },
  {
    to: '/archived',
    label: '已归档事项',
    icon: Archive,
    count: tasks.counts.archived,
  },
  { to: '/trash', label: '回收站', icon: Trash2, count: tasks.counts.trash },
])

const isActive = (path: string) => route.path === path
</script>

<template>
  <aside
    class="flex w-[206px] shrink-0 flex-col border-r border-line bg-panel-2"
  >
    <!-- 品牌 -->
    <div class="flex items-center gap-2.5 px-3 pb-3.5 pt-3.5">
      <div
        class="grid h-7 w-7 shrink-0 place-items-center rounded-[7px] bg-accent text-white"
      >
        <ListTodo :size="16" :stroke-width="2.3" />
      </div>
      <div class="min-w-0 leading-tight">
        <div
          class="truncate text-[13.5px] font-semibold tracking-tight text-ink"
        >
          ShiftNote
        </div>
        <div class="truncate text-[10.5px] text-ink-3">交接班 · 事项跟踪</div>
      </div>
    </div>

    <nav
      class="min-h-0 flex-1 overflow-y-auto px-2"
      :class="{ 'pointer-events-none opacity-60': tasks.batchBusy }"
      :aria-busy="tasks.batchBusy"
    >
      <router-link
        v-for="item in primaryItems"
        :key="item.to"
        :to="item.to"
        class="nav-item"
        :class="{ 'is-active': isActive(item.to) }"
      >
        <component
          :is="item.icon"
          :size="15"
          :stroke-width="2"
          class="shrink-0"
        />
        <span class="flex-1 truncate">{{ item.label }}</span>
        <span
          v-if="item.count > 0"
          class="tabular-nums"
          :class="item.emphasis ? 'font-semibold text-p0' : 'text-ink-3'"
          style="font-size: 10.5px"
        >
          {{ item.count }}
        </span>
      </router-link>

      <div class="divider my-2.5" />

      <router-link
        v-for="item in secondaryItems"
        :key="item.to"
        :to="item.to"
        class="nav-item"
        :class="{ 'is-active': isActive(item.to) }"
      >
        <component
          :is="item.icon"
          :size="15"
          :stroke-width="2"
          class="shrink-0"
        />
        <span class="flex-1 truncate">{{ item.label }}</span>
        <span
          v-if="item.count > 0"
          class="tabular-nums text-ink-3"
          style="font-size: 10.5px"
        >
          {{ item.count }}
        </span>
      </router-link>

      <div class="divider my-2.5" />

      <router-link
        to="/handovers"
        class="nav-item"
        :class="{ 'is-active': route.path.startsWith('/handovers') }"
      >
        <ClipboardList :size="15" /><span class="flex-1">交接记录</span>
      </router-link>

      <button class="nav-item w-full" type="button" @click="ui.openCreate()">
        <span
          class="grid h-[15px] w-[15px] place-items-center rounded border border-dashed border-line-strong text-ink-3"
        >
          +
        </span>
        <span class="flex-1 truncate">快速添加事项</span>
        <span class="font-mono text-[10px]" style="color: var(--sn-ink-3)"
          >Ctrl N</span
        >
      </button>
    </nav>

    <div class="space-y-0.5 border-t border-line px-2 py-2">
      <router-link
        to="/settings"
        class="nav-item"
        :class="{ 'is-active': isActive('/settings') }"
      >
        <Settings :size="15" :stroke-width="2" />
        <span class="flex-1">设置</span>
      </router-link>

      <button class="nav-item w-full" type="button" @click="ui.toggleTheme()">
        <Moon v-if="ui.theme === 'light'" :size="15" :stroke-width="2" />
        <Sun v-else :size="15" :stroke-width="2" />
        <span class="flex-1 text-left">{{
          ui.theme === 'light' ? '切换到深色' : '切换到浅色'
        }}</span>
      </button>
    </div>
  </aside>
</template>
