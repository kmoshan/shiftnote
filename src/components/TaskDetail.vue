<script setup lang="ts">
import { computed } from 'vue'
import {
  Check,
  Archive,
  ArchiveRestore,
  CheckCircle2,
  Copy,
  Eye,
  EyeOff,
  Pencil,
  RotateCcw,
  Trash2,
  X,
} from '@lucide/vue'

import ProgressTimeline from '@/components/ProgressTimeline.vue'
import { formatDateTime, formatSmart } from '@/services/format'
import { useTaskStore } from '@/stores/tasks'
import { useUiStore } from '@/stores/ui'
import { priorityMeta } from '@/types/priority'

const tasks = useTaskStore()
const ui = useUiStore()

const task = computed(() => tasks.detail?.task ?? null)
const updates = computed(() =>
  task.value
    ? (tasks.updatesByTask[task.value.id] ?? tasks.detail?.updates ?? [])
    : []
)
const meta = computed(() => priorityMeta(task.value?.priority ?? 2))
const isDone = computed(() => task.value?.status === 'done')
const inTrash = computed(() => Boolean(task.value?.deletedAt))
const busy = computed(
  () =>
    tasks.batchBusy ||
    tasks.detailLoading ||
    (task.value ? Boolean(tasks.busy[task.value.id]) : false)
)

const rows = computed(() => {
  const current = task.value
  if (!current) return []
  return [
    { label: '记录时间', value: formatDateTime(current.recordedAt) },
    { label: '负责部门', value: current.department ?? '—' },
    { label: '创建时间', value: formatDateTime(current.createdAt) },
    { label: '最近更新', value: formatSmart(current.updatedAt) },
  ]
})

function close() {
  void tasks.selectTask(null)
}
</script>

<template>
  <aside class="flex w-[406px] shrink-0 flex-col border-l border-line bg-panel">
    <div
      class="flex h-11 shrink-0 items-center gap-2 border-b border-line px-3"
    >
      <span
        class="text-[11.5px] font-semibold uppercase tracking-wide text-ink-3"
        >事项详情</span
      >
      <button
        type="button"
        class="ml-auto grid h-7 w-7 place-items-center rounded text-ink-3 hover:bg-hover hover:text-ink"
        title="关闭详情 (Esc)"
        @click="close"
      >
        <X :size="15" />
      </button>
    </div>

    <div
      v-if="!task"
      class="flex flex-1 items-center justify-center text-[12px] text-ink-3"
    >
      正在读取事项…
    </div>

    <div v-else class="min-h-0 flex-1 overflow-y-auto pb-6">
      <div class="px-4 pt-3.5">
        <div class="flex flex-wrap items-center gap-1.5">
          <span class="chip" :class="meta.chipClass"
            >{{ meta.code }} {{ meta.label }}</span
          >
          <span v-if="isDone" class="chip chip-p2">已完成</span>
          <span v-if="task.hidden" class="chip chip-p3">已隐藏</span>
          <span v-if="task.archived" class="chip chip-p3">已归档</span>
          <span v-if="inTrash" class="chip chip-p0">回收站</span>
          <span class="ml-auto text-[10.5px] text-ink-3"
            >{{ task.updateCount }} 条进展</span
          >
        </div>

        <h2
          class="mt-2 text-[15px] font-semibold leading-snug"
          :class="isDone ? 'text-ink-3 line-through' : 'text-ink'"
        >
          {{ task.title }}
        </h2>

        <!-- 操作区 -->
        <fieldset
          class="mt-2.5 flex flex-wrap items-center gap-1.5"
          :disabled="busy"
        >
          <template v-if="inTrash">
            <button
              type="button"
              class="btn h-7"
              @click="tasks.restoreTask(task)"
            >
              <RotateCcw :size="12" />
              恢复
            </button>
            <button
              type="button"
              class="btn btn-danger h-7"
              @click="tasks.deletePermanently(task)"
            >
              <Trash2 :size="12" />
              永久删除
            </button>
          </template>

          <template v-else>
            <button
              type="button"
              class="btn h-7"
              :class="isDone ? '' : 'btn-primary'"
              :disabled="busy"
              @click="tasks.toggleCompleted(task)"
            >
              <CheckCircle2 :size="12" />
              {{ isDone ? '取消完成' : '标记完成' }}
            </button>
            <button type="button" class="btn h-7" @click="ui.openEdit(task)">
              <Pencil :size="12" />
              编辑
            </button>
            <button
              type="button"
              class="btn h-7"
              @click="tasks.duplicateTask(task)"
            >
              <Copy :size="12" />
              复制
            </button>
            <button
              v-if="task.archived"
              type="button"
              class="btn h-7"
              @click="tasks.archiveTask(task, false)"
            >
              <ArchiveRestore :size="12" />取消归档
            </button>
            <button
              v-else-if="isDone"
              type="button"
              class="btn h-7"
              @click="tasks.archiveTask(task)"
            >
              <Archive :size="12" />归档
            </button>
            <button
              v-if="!task.hidden"
              type="button"
              class="btn h-7"
              @click="tasks.hideTask(task)"
            >
              <EyeOff :size="12" />
              隐藏
            </button>
            <button
              v-else
              type="button"
              class="btn h-7"
              @click="tasks.unhideTask(task)"
            >
              <Eye :size="12" />
              取消隐藏
            </button>
            <button
              type="button"
              class="btn btn-danger h-7"
              @click="tasks.trashTask(task)"
            >
              <Trash2 :size="12" />
              删除
            </button>
          </template>
        </fieldset>
      </div>

      <!-- 字段 -->
      <dl class="mt-3.5 grid grid-cols-2 gap-x-3 gap-y-2.5 px-4">
        <div v-for="row in rows" :key="row.label" class="min-w-0">
          <dt
            class="text-[10.5px] font-medium uppercase tracking-wide text-ink-3"
          >
            {{ row.label }}
          </dt>
          <dd class="truncate text-[12px] text-ink-2" :title="row.value">
            {{ row.value }}
          </dd>
        </div>
      </dl>

      <!-- 描述 -->
      <div v-if="task.description" class="mt-4 px-4">
        <h3
          class="mb-1 text-[10.5px] font-semibold uppercase tracking-wide text-ink-3"
        >
          详细描述
        </h3>
        <p
          class="whitespace-pre-wrap break-words rounded-md bg-panel-2 p-2.5 text-[12.5px] leading-relaxed text-ink-2"
        >
          {{ task.description }}
        </p>
      </div>

      <div class="mt-4 border-t border-line pt-3">
        <ProgressTimeline
          :task-id="task.id"
          :updates="updates"
          :disabled="inTrash || tasks.detailLoading || tasks.batchBusy"
        />
      </div>

      <div class="mt-2 flex items-center gap-1.5 px-4 text-[10.5px] text-ink-3">
        <Check :size="11" />
        所有进展均保存在本地数据库，编辑不会覆盖历史时间
      </div>
    </div>
  </aside>
</template>
