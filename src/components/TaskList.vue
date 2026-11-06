<script setup lang="ts">
import { computed, ref } from 'vue'
import { AlertTriangle, ChevronLeft, ChevronRight } from '@lucide/vue'
import EmptyState from '@/components/EmptyState.vue'
import TaskItem from '@/components/TaskItem.vue'
import TaskQuickAdd from '@/components/TaskQuickAdd.vue'
import TaskFilters from '@/components/TaskFilters.vue'
import TaskBatchBar from '@/components/TaskBatchBar.vue'
import { useTaskStore } from '@/stores/tasks'
import { VIEW_HINTS, VIEW_LABELS } from '@/types/priority'
const tasks = useTaskStore()
const title = computed(() =>
  tasks.isSearching
    ? tasks.view === 'today'
      ? '今日搜索结果'
      : '搜索结果'
    : (VIEW_LABELS[tasks.view] ?? '事项列表')
)
const hint = computed(() =>
  tasks.isSearching
    ? `关键词「${tasks.search.trim()}」 · 同时匹配标题、描述、负责部门与全部历史进展`
    : (VIEW_HINTS[tasks.view] ?? '')
)
const canQuickAdd = computed(
  () => !['trash', 'hidden', 'done', 'archived'].includes(tasks.view)
)
const dragId = ref<string | null>(null)
const dropId = ref<string | null>(null)
function dragStart(id: string, event: DragEvent) {
  if (!tasks.canReorder || !event.dataTransfer) {
    event.preventDefault()
    return
  }
  dragId.value = id
  event.dataTransfer.effectAllowed = 'move'
  event.dataTransfer.setData('text/plain', id)
  const row = (event.target as HTMLElement).closest('.task-row')
  if (row) event.dataTransfer.setDragImage(row, 30, 20)
}
function dragOver(id: string, event: DragEvent) {
  if (!tasks.canReorder || !dragId.value || dragId.value === id) return
  event.preventDefault()
  dropId.value = id
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
}
async function drop(id: string, event: DragEvent) {
  event.preventDefault()
  const source = dragId.value
  dragId.value = null
  dropId.value = null
  if (!source || source === id || !tasks.canReorder) return
  // afterId is the item's successor in the complete view, even across pages.
  await tasks.moveTask(source, { afterId: id })
}
function endDrag() {
  dragId.value = null
  dropId.value = null
}
function pageSizeChange(event: Event) {
  void tasks.setPageSize(Number((event.target as HTMLSelectElement).value))
}
</script>
<template>
  <section class="flex min-h-0 flex-1 flex-col">
    <div
      class="flex flex-wrap items-center gap-x-2 gap-y-1.5 px-3.5 pb-1.5 pt-3"
    >
      <h2 class="shrink-0 whitespace-nowrap text-[13px] font-semibold text-ink">
        {{ title }}
      </h2>
      <span
        class="shrink-0 rounded-full bg-panel-2 px-1.5 py-[1px] text-[10.5px] tabular-nums text-ink-3"
        >共 {{ tasks.total }} 条 · 本页 {{ tasks.tasks.length }} 条</span
      >
      <span
        class="hidden min-w-0 flex-1 truncate text-[11.5px] text-ink-3 xl:inline"
        >{{ hint }}</span
      >
    </div>
    <div class="px-3.5 pb-2"><TaskFilters /></div>
    <div
      v-if="tasks.isSearching"
      class="mx-3.5 mb-2 flex items-center gap-2 text-[11.5px] text-ink-3"
    >
      <span>关键词：{{ tasks.search.trim() }}</span
      ><button
        type="button"
        class="underline"
        :disabled="tasks.batchBusy"
        @click="tasks.setSearch('')"
      >
        清除搜索
      </button>
    </div>
    <div
      v-if="tasks.error"
      class="mx-3.5 mb-1.5 flex items-start gap-2 rounded-md bg-p0-soft px-2.5 py-1.5"
    >
      <AlertTriangle :size="13" class="mt-[2px] shrink-0 text-p0" />
      <p class="flex-1 text-[11.5px] leading-relaxed text-p0">
        {{ tasks.error }}
      </p>
      <button
        type="button"
        class="text-[11px] font-medium text-p0 underline"
        :disabled="tasks.batchBusy"
        @click="tasks.refresh()"
      >
        重试
      </button>
    </div>
    <TaskBatchBar />
    <TaskQuickAdd v-if="canQuickAdd && !tasks.batchBusy" />
    <div
      class="min-h-0 flex-1 overflow-y-auto px-3 pb-4"
      :aria-busy="tasks.loading"
    >
      <div
        v-if="tasks.loading && tasks.tasks.length === 0"
        class="flex flex-col gap-2 py-3"
      >
        <div
          v-for="index in 3"
          :key="index"
          class="h-[62px] animate-pulse rounded-lg bg-panel-2"
        />
        <p class="text-center text-[11.5px] text-ink-3">正在读取本地数据…</p>
      </div>
      <TaskItem
        v-for="task in tasks.tasks"
        :key="task.id"
        :task="task"
        :class="{ 'ring-2 ring-accent': dropId === task.id }"
        @dragstart="dragStart(task.id, $event)"
        @dragover="dragOver(task.id, $event)"
        @drop="drop(task.id, $event)"
        @dragend="endDrag"
      />
      <EmptyState
        v-if="!tasks.loading && tasks.tasks.length === 0"
        :view="tasks.view"
        :searching="tasks.isSearching"
      />
    </div>
    <div
      class="flex flex-wrap items-center justify-end gap-2 border-t border-line px-3.5 py-2 text-[11.5px] text-ink-3"
    >
      <span v-if="tasks.loading" class="mr-auto" role="status">正在读取…</span>
      <label class="flex items-center gap-1"
        >每页<select
          :value="tasks.pageSize"
          class="input h-7 w-16 px-1 text-[11.5px]"
          :disabled="tasks.batchBusy || tasks.loading"
          aria-label="每页条数"
          @change="pageSizeChange"
        >
          <option :value="50">50</option>
          <option :value="100">100</option>
          <option :value="200">200</option>
          <option :value="500">500</option></select
        >条</label
      >
      <button
        type="button"
        class="btn h-7 px-2"
        :disabled="tasks.page <= 1 || tasks.loading || tasks.batchBusy"
        @click="tasks.setPage(tasks.page - 1)"
      >
        <ChevronLeft :size="12" />上一页
      </button>
      <span class="tabular-nums"
        >第 {{ tasks.page }} / {{ tasks.pageCount }} 页</span
      >
      <button
        type="button"
        class="btn h-7 px-2"
        :disabled="
          tasks.page >= tasks.pageCount || tasks.loading || tasks.batchBusy
        "
        @click="tasks.setPage(tasks.page + 1)"
      >
        下一页<ChevronRight :size="12" />
      </button>
    </div>
  </section>
</template>
