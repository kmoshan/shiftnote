<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import { ArrowDownUp, Check, Pencil, Send, Trash2, X } from '@lucide/vue'

import { formatSmart } from '@/services/format'
import { useTaskStore } from '@/stores/tasks'
import type { TaskUpdate } from '@/types'

const props = defineProps<{
  taskId: string
  updates: TaskUpdate[]
  disabled?: boolean
}>()

const tasks = useTaskStore()

const state = computed(() => tasks.getProgressDraft(props.taskId))
const draft = computed({
  get: () => state.value.content,
  set: (value) => {
    state.value.content = value
  },
})
const author = computed({
  get: () => state.value.author,
  set: (value) => {
    tasks.setProgressAuthor(props.taskId, value)
  },
})
const saving = computed(() => state.value.saving)
const composerRef = ref<HTMLTextAreaElement | null>(null)

const editingId = computed(() => state.value.editingId)
const editingText = computed({
  get: () => state.value.editingText,
  set: (value) => {
    state.value.editingText = value
  },
})
const editingSaving = computed(() => state.value.editingSaving)

const newestFirst = ref(true)
const ordered = computed(() =>
  newestFirst.value ? props.updates : [...props.updates].reverse()
)

async function submit() {
  const content = draft.value.trim()
  if (!content || saving.value || props.disabled || tasks.batchBusy) return
  const taskId = props.taskId
  const captured = state.value
  const original = captured.content
  const submittedAuthor = captured.author.trim()
  captured.saving = true
  const created = await tasks.addProgress(
    taskId,
    content,
    submittedAuthor || null
  )
  captured.saving = false

  if (created) {
    if (captured.content === original) captured.content = ''
    await nextTick()
    if (props.taskId === taskId) composerRef.value?.focus()
  }
}

function startEdit(item: TaskUpdate) {
  if (props.disabled || state.value.editingSaving || tasks.batchBusy) return
  state.value.editingId = item.id
  state.value.editingText = item.content
  void nextTick(() => {
    const element = document.getElementById(
      `update-editor-${item.id}`
    ) as HTMLTextAreaElement | null
    element?.focus()
    element?.setSelectionRange(element.value.length, element.value.length)
  })
}

function cancelEdit() {
  state.value.editingId = null
  state.value.editingText = ''
}

async function saveEdit(item: TaskUpdate) {
  const content = editingText.value.trim()
  if (!content || editingSaving.value || props.disabled || tasks.batchBusy)
    return
  const taskId = props.taskId
  const captured = state.value
  const original = captured.editingText
  captured.editingSaving = true
  const updated = await tasks.editProgress(taskId, item.id, content)
  captured.editingSaving = false
  if (
    updated &&
    captured.editingId === item.id &&
    captured.editingText === original
  ) {
    captured.editingId = null
    captured.editingText = ''
  }
}
</script>

<template>
  <section class="flex min-h-0 flex-col">
    <div class="flex items-center gap-2 px-4">
      <h3 class="text-[11px] font-semibold uppercase tracking-wide text-ink-3">
        处理进展
        <span class="ml-1 font-normal normal-case tabular-nums"
          >{{ updates.length }} 条</span
        >
      </h3>
      <button
        type="button"
        class="ml-auto flex items-center gap-1 rounded px-1.5 py-0.5 text-[10.5px] text-ink-3 hover:bg-hover hover:text-ink"
        :title="newestFirst ? '切换为最早在前' : '切换为最新在前'"
        @click="newestFirst = !newestFirst"
      >
        <ArrowDownUp :size="11" />
        {{ newestFirst ? '最新在前' : '最早在前' }}
      </button>
    </div>

    <!-- 追加进展 -->
    <div v-if="!disabled" class="mt-2 px-4">
      <div class="rounded-lg border border-line bg-panel-2 p-2">
        <textarea
          ref="composerRef"
          v-model="draft"
          class="input text-[12.5px]"
          rows="2"
          placeholder="记录这次处理了什么…（Ctrl+Enter 保存）"
          @keydown.ctrl.enter.prevent="submit"
        />
        <div class="mt-1.5 flex items-center gap-1.5">
          <input
            v-model="author"
            class="input h-7 w-[110px] text-[11.5px]"
            placeholder="处理人（选填）"
          />
          <span class="hidden truncate text-[10.5px] text-ink-3 sm:inline">
            每条进展都会保留时间，不会被覆盖
          </span>
          <button
            type="button"
            class="btn btn-primary ml-auto h-7"
            :disabled="!draft.trim() || saving || tasks.batchBusy"
            @click="submit"
          >
            <Send :size="12" />
            保存进展
          </button>
        </div>
      </div>
    </div>

    <!-- 时间线 -->
    <div
      v-if="ordered.length === 0"
      class="mt-3 px-4 pb-2 text-[11.5px] leading-relaxed text-ink-3"
    >
      还没有进展记录。每次交接前追加一条进展，就能形成完整的处理历史。
    </div>

    <ol
      v-else
      class="relative mt-3 ml-[18px] border-l border-line pb-1 pl-4 pr-4"
    >
      <li
        v-for="(item, index) in ordered"
        :key="item.id"
        class="group relative pb-3.5 last:pb-1"
      >
        <span
          class="absolute -left-[19.5px] top-[4px] h-[7px] w-[7px] rounded-full ring-2 ring-panel"
          :class="index === 0 && newestFirst ? 'bg-accent' : 'bg-line-strong'"
        />

        <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5">
          <span class="text-[11.5px] font-medium tabular-nums text-ink-2">
            {{ formatSmart(item.createdAt) }}
          </span>
          <span v-if="item.author" class="chip chip-p3">{{ item.author }}</span>
          <span v-if="item.editedAt" class="text-[10.5px] text-ink-3">
            已编辑 {{ formatSmart(item.editedAt) }}
          </span>
        </div>

        <template v-if="editingId === item.id">
          <textarea
            :id="`update-editor-${item.id}`"
            v-model="editingText"
            class="input mt-1.5 text-[12.5px]"
            rows="2"
            @keydown.ctrl.enter.prevent="saveEdit(item)"
            @keydown.esc.prevent="cancelEdit"
          />
          <div class="mt-1 flex justify-end gap-1.5">
            <button
              type="button"
              class="btn h-6 px-2 text-[11px]"
              @click="cancelEdit"
            >
              <X :size="11" />
              取消
            </button>
            <button
              type="button"
              class="btn btn-primary h-6 px-2 text-[11px]"
              :disabled="
                !editingText.trim() ||
                editingSaving ||
                disabled ||
                tasks.batchBusy
              "
              @click="saveEdit(item)"
            >
              <Check :size="11" />
              保存修改
            </button>
          </div>
        </template>

        <p
          v-else
          class="mt-1 whitespace-pre-wrap break-words text-[12.5px] leading-relaxed text-ink"
        >
          {{ item.content }}
        </p>

        <div
          v-if="editingId !== item.id && !disabled"
          class="absolute right-0 top-0 flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100"
        >
          <button
            type="button"
            class="grid h-6 w-6 place-items-center rounded text-ink-3 hover:bg-hover hover:text-ink"
            title="修改这条进展"
            @click="startEdit(item)"
          >
            <Pencil :size="12" />
          </button>
          <button
            type="button"
            class="grid h-6 w-6 place-items-center rounded text-ink-3 hover:bg-p0-soft hover:text-p0"
            title="删除这条进展"
            @click="tasks.removeProgress(props.taskId, item)"
          >
            <Trash2 :size="12" />
          </button>
        </div>
      </li>
    </ol>
  </section>
</template>
