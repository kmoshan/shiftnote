<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import {
  Check,
  Archive,
  ArchiveRestore,
  ChevronDown,
  ChevronUp,
  CornerDownLeft,
  Copy,
  Eye,
  EyeOff,
  GripVertical,
  MoreHorizontal,
  Pencil,
  Plus,
  RotateCcw,
  Send,
  Trash2,
} from '@lucide/vue'

import { formatSmart } from '@/services/format'
import { useTaskStore } from '@/stores/tasks'
import { useUiStore } from '@/stores/ui'
import type { Task } from '@/types'
import { PRIORITIES, priorityMeta } from '@/types/priority'

const props = defineProps<{ task: Task }>()

const tasks = useTaskStore()
const ui = useUiStore()

const meta = computed(() => priorityMeta(props.task.priority))
const isDone = computed(() => props.task.status === 'done')
const isSelected = computed(() => tasks.selectedId === props.task.id)
const isBusy = computed(
  () => tasks.batchBusy || Boolean(tasks.busy[props.task.id])
)
const inTrash = computed(() => props.task.deletedAt !== null)

const updatedAtLabel = computed(() => formatSmart(props.task.updatedAt))
const recordedAtLabel = computed(() => formatSmart(props.task.recordedAt))

const composerOpen = ref(false)
const progressDraft = computed(() => tasks.getProgressDraft(props.task.id))
const progressText = computed({
  get: () => progressDraft.value.content,
  set: (value) => {
    progressDraft.value.content = value
  },
})
const progressSaving = computed(() => progressDraft.value.saving)
const composerRef = ref<HTMLTextAreaElement | null>(null)
const menuOpen = ref(false)

// 行内历史进展：默认只看最新一条，展开后按需加载全部
const historyOpen = ref(false)
const historyLoading = computed(() =>
  Boolean(tasks.updatesLoading[props.task.id])
)
const historyItems = computed(() => tasks.updatesByTask[props.task.id] ?? [])

async function reloadHistory() {
  await tasks.loadUpdates(props.task.id)
}

async function toggleHistory() {
  if (historyOpen.value) {
    historyOpen.value = false
    return
  }
  historyOpen.value = true
  await reloadHistory()
}

async function openComposer() {
  if (isBusy.value) return
  composerOpen.value = true
  await nextTick()
  composerRef.value?.focus()
}

function closeComposer() {
  composerOpen.value = false
  progressText.value = ''
}

async function saveProgress() {
  const value = progressText.value.trim()
  if (!value || progressSaving.value || isBusy.value) return
  const taskId = props.task.id
  const captured = progressDraft.value
  const original = captured.content
  captured.saving = true
  const created = await tasks.addProgress(taskId, value)
  captured.saving = false
  if (created) {
    if (captured.content === original) captured.content = ''
    if (props.task.id === taskId) composerOpen.value = false
  }
}

function select() {
  void tasks.selectTask(props.task.id)
}

function toggleDone() {
  void tasks.toggleCompleted(props.task)
}

function changePriority(priority: number) {
  menuOpen.value = false
  void tasks.changePriority(props.task.id, priority)
}

function runMenuAction(action: () => unknown) {
  menuOpen.value = false
  void action()
}

function closeMenuOnOutside(event: MouseEvent) {
  if (!menuOpen.value) return
  const target = event.target as HTMLElement | null
  if (target && target.closest('[data-task-menu]')) return
  menuOpen.value = false
}

onMounted(() => document.addEventListener('mousedown', closeMenuOnOutside))
onUnmounted(() => document.removeEventListener('mousedown', closeMenuOnOutside))
</script>

<template>
  <article
    class="task-row relative mb-1.5 rounded-lg border bg-panel transition-colors"
    :class="[
      isSelected
        ? 'border-accent shadow-[0_0_0_2px_var(--sn-focus-ring)]'
        : 'border-line hover:border-line-strong',
      isBusy ? 'opacity-60' : '',
      inTrash ? 'bg-panel-2' : '',
    ]"
  >
    <div class="flex items-start gap-2.5 px-3 py-2.5">
      <input
        type="checkbox"
        class="mt-[3px] h-3.5 w-3.5 shrink-0 accent-accent"
        :aria-label="`选择事项：${task.title}`"
        title="选择用于批量操作（不改变完成状态）"
        :checked="tasks.selectedIds.includes(task.id)"
        :disabled="tasks.batchBusy || tasks.loading"
        @click.stop
        @change="tasks.toggleSelection(task.id)"
      />
      <button
        v-if="tasks.sort === 'manual'"
        type="button"
        class="mt-[1px] cursor-grab text-ink-3 disabled:cursor-default disabled:opacity-30"
        :draggable="tasks.canReorder && !isBusy"
        :disabled="!tasks.canReorder || isBusy"
        :title="tasks.reorderReason || '拖动以手动排序'"
        aria-label="拖动以手动排序"
        @click.stop
      >
        <GripVertical :size="15" />
      </button>
      <!-- 完成勾选框 -->
      <button
        type="button"
        class="mt-[2px] grid h-[17px] w-[17px] shrink-0 place-items-center rounded-[4px] border transition-colors"
        :class="
          isDone
            ? 'border-accent bg-accent text-white'
            : 'border-line-strong bg-panel text-transparent hover:border-accent hover:text-accent'
        "
        :title="isDone ? '取消完成' : '标记完成'"
        :disabled="isBusy || inTrash"
        @click.stop="toggleDone"
      >
        <Check :size="12" :stroke-width="3" />
      </button>

      <!-- 主体 -->
      <div
        class="min-w-0 flex-1"
        @click="select"
        @dblclick.stop="ui.openEdit(task)"
      >
        <div class="flex items-center gap-2">
          <span class="chip shrink-0" :class="meta.chipClass">
            {{ meta.code }} {{ meta.label }}
          </span>

          <h3
            class="truncate text-[13.5px] font-medium"
            :class="isDone ? 'text-ink-3 line-through' : 'text-ink'"
            :title="task.title"
          >
            {{ task.title }}
          </h3>

          <span v-if="task.hidden" class="chip chip-p3 shrink-0">已隐藏</span>
          <span v-if="task.archived" class="chip chip-p3 shrink-0">已归档</span>
          <span v-if="isDone" class="chip chip-p2 shrink-0">已完成</span>
        </div>

        <div
          class="mt-[3px] flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-[11.5px] text-ink-3"
        >
          <span class="whitespace-nowrap">记录 {{ recordedAtLabel }}</span>
          <span class="opacity-50">·</span>
          <span class="whitespace-nowrap">更新 {{ updatedAtLabel }}</span>
          <template v-if="task.department">
            <span class="opacity-50">·</span>
            <span class="whitespace-nowrap">部门 {{ task.department }}</span>
          </template>
          <template v-if="task.updateCount > 0">
            <span class="opacity-50">·</span>
            <span class="whitespace-nowrap">{{ task.updateCount }} 条进展</span>
          </template>
        </div>

        <div
          v-if="task.updateCount > 0"
          class="mt-1.5 rounded-md bg-panel-2 px-2 py-1 text-[12px] text-ink-2"
        >
          <div class="flex items-start gap-1.5">
            <CornerDownLeft
              :size="12"
              :stroke-width="2"
              class="mt-[3px] shrink-0 text-ink-3"
            />

            <span v-if="!historyOpen" class="line-clamp-2 min-w-0 flex-1">
              {{ task.latestUpdateContent }}
            </span>
            <span v-else class="min-w-0 flex-1 text-[11.5px] text-ink-3">
              共 {{ task.updateCount }} 条进展，最新在前
            </span>

            <button
              v-if="task.updateCount > 1 || historyOpen"
              type="button"
              class="ml-1 flex shrink-0 items-center gap-0.5 whitespace-nowrap rounded px-1 text-[10.5px] text-ink-3 transition-colors hover:bg-hover hover:text-accent"
              :title="historyOpen ? '收起历史进展' : '展开该事项的全部历史进展'"
              @click.stop="toggleHistory"
            >
              <template v-if="historyOpen">
                收起
                <ChevronUp :size="11" />
              </template>
              <template v-else>
                展开 {{ task.updateCount }} 条
                <ChevronDown :size="11" />
              </template>
            </button>
          </div>

          <div v-if="historyOpen" class="mt-1.5 border-t border-line pt-1.5">
            <div v-if="historyLoading" class="text-[11px] text-ink-3">
              正在读取历史进展…
            </div>
            <ol v-else class="space-y-1.5">
              <li
                v-for="(item, index) in historyItems"
                :key="item.id"
                class="text-[11.5px] leading-relaxed"
              >
                <div class="flex items-center gap-1.5 text-[10.5px] text-ink-3">
                  <span class="tabular-nums">{{
                    formatSmart(item.createdAt)
                  }}</span>
                  <span
                    v-if="index === 0"
                    class="chip chip-p2 px-1"
                    style="height: 15px; font-size: 9.5px"
                  >
                    最新
                  </span>
                  <span
                    v-if="item.author"
                    class="chip chip-p3 px-1"
                    style="height: 15px; font-size: 9.5px"
                  >
                    {{ item.author }}
                  </span>
                  <span v-if="item.editedAt" class="text-[9.5px]">已编辑</span>
                </div>
                <p class="whitespace-pre-wrap break-words text-ink-2">
                  {{ item.content }}
                </p>
              </li>
            </ol>
          </div>
        </div>
      </div>

      <!-- 悬停操作 -->
      <fieldset
        class="row-actions ml-1 flex shrink-0 items-center gap-1"
        :disabled="isBusy"
      >
        <button
          v-if="inTrash"
          type="button"
          class="btn h-7 px-2 text-[11.5px]"
          @click.stop="tasks.restoreTask(task)"
        >
          <RotateCcw :size="12" />
          恢复
        </button>
        <button
          v-else
          type="button"
          class="btn h-7 px-2 text-[11.5px]"
          title="追加进展"
          @click.stop="openComposer"
        >
          <Plus :size="12" :stroke-width="2.4" />
          追加进展
        </button>

        <button
          type="button"
          class="btn btn-ghost h-7 w-7 px-0"
          title="更多操作"
          data-task-menu
          @click.stop="menuOpen = !menuOpen"
        >
          <MoreHorizontal :size="15" />
        </button>
      </fieldset>
    </div>

    <!-- 行内追加进展 -->
    <div v-if="composerOpen" class="border-t border-line px-3 py-2" @click.stop>
      <textarea
        ref="composerRef"
        v-model="progressText"
        class="input text-[12.5px]"
        rows="2"
        placeholder="记录这次处理了什么…（Ctrl+Enter 保存，Esc 取消）"
        @keydown.ctrl.enter.prevent="saveProgress"
        @keydown.esc.stop.prevent="closeComposer"
      />
      <div class="mt-1.5 flex items-center justify-end gap-1.5">
        <span class="mr-auto text-[10.5px] text-ink-3"
          >进展只追加，不会覆盖历史记录</span
        >
        <button class="btn h-7" type="button" @click="closeComposer">
          取消
        </button>
        <button
          class="btn btn-primary h-7"
          type="button"
          :disabled="!progressText.trim() || progressSaving || isBusy"
          @click="saveProgress"
        >
          <Send :size="12" />
          保存进展
        </button>
      </div>
    </div>

    <!-- 更多菜单 -->
    <fieldset
      v-if="menuOpen"
      :disabled="isBusy"
      data-task-menu
      class="absolute right-2 top-[38px] z-30 w-[188px] rounded-lg border border-line bg-panel py-1 shadow-[var(--sn-shadow-pop)]"
      @click.stop
    >
      <div
        class="px-2.5 pb-1 pt-1 text-[10.5px] font-medium uppercase tracking-wide text-ink-3"
      >
        调整优先级
      </div>
      <div class="flex gap-1 px-2 pb-1.5">
        <button
          v-for="option in PRIORITIES"
          :key="option.value"
          type="button"
          class="chip flex-1 justify-center"
          :class="[
            option.chipClass,
            task.priority === option.value ? 'ring-1 ring-current' : '',
          ]"
          @click="changePriority(option.value)"
        >
          {{ option.code }}
        </button>
      </div>
      <div class="divider my-1" />

      <button
        class="menu-item"
        type="button"
        @click="runMenuAction(() => ui.openEdit(task))"
      >
        <Pencil :size="13" />
        编辑事项
      </button>
      <button
        class="menu-item"
        type="button"
        @click="runMenuAction(() => tasks.duplicateTask(task))"
      >
        <Copy :size="13" />
        复制事项
      </button>

      <template v-if="inTrash">
        <button
          class="menu-item"
          type="button"
          @click="runMenuAction(() => tasks.restoreTask(task))"
        >
          <RotateCcw :size="13" />
          恢复到列表
        </button>
        <button
          class="menu-item is-danger"
          type="button"
          @click="runMenuAction(() => tasks.deletePermanently(task))"
        >
          <Trash2 :size="13" />
          永久删除
        </button>
      </template>

      <template v-else>
        <button
          v-if="task.archived"
          class="menu-item"
          type="button"
          @click="runMenuAction(() => tasks.archiveTask(task, false))"
        >
          <ArchiveRestore :size="13" />取消归档
        </button>
        <button
          v-else-if="isDone"
          class="menu-item"
          type="button"
          @click="runMenuAction(() => tasks.archiveTask(task))"
        >
          <Archive :size="13" />归档事项
        </button>
        <button
          v-if="task.hidden"
          class="menu-item"
          type="button"
          @click="runMenuAction(() => tasks.unhideTask(task))"
        >
          <Eye :size="13" />
          取消隐藏
        </button>
        <button
          v-else
          class="menu-item"
          type="button"
          @click="runMenuAction(() => tasks.hideTask(task))"
        >
          <EyeOff :size="13" />
          隐藏事项
        </button>
        <button
          class="menu-item is-danger"
          type="button"
          @click="runMenuAction(() => tasks.trashTask(task))"
        >
          <Trash2 :size="13" />
          移入回收站
        </button>
      </template>
      <template v-if="tasks.canReorder">
        <div class="divider my-1" />
        <button
          class="menu-item"
          type="button"
          :disabled="tasks.page <= 1"
          @click="runMenuAction(() => tasks.moveToAdjacentPage(task.id, -1))"
        >
          <ChevronUp :size="13" />移到上一页
        </button>
        <button
          class="menu-item"
          type="button"
          :disabled="tasks.page >= tasks.pageCount"
          @click="runMenuAction(() => tasks.moveToAdjacentPage(task.id, 1))"
        >
          <ChevronDown :size="13" />移到下一页
        </button>
      </template>
    </fieldset>
  </article>
</template>
