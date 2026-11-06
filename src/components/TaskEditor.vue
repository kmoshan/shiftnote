<script setup lang="ts">
import { computed, nextTick, onMounted, reactive, ref } from 'vue'
import { Clock, CornerDownLeft, X } from '@lucide/vue'

import { fromDatetimeInput, toDatetimeInput } from '@/services/format'
import { useTaskStore } from '@/stores/tasks'
import { useToastStore } from '@/stores/toast'
import { useUiStore } from '@/stores/ui'
import type { CreateTaskInput } from '@/types'
import { PRIORITIES } from '@/types/priority'

const ui = useUiStore()
const tasks = useTaskStore()
const toast = useToastStore()

const saving = ref(false)
const titleRef = ref<HTMLInputElement | null>(null)

const form = reactive({
  title: '',
  description: '',
  priority: 2,
  department: '',
  recordedAt: '',
})

const isEdit = computed(() => ui.editingTask !== null)

function nowInputValue(): string {
  return toDatetimeInput(Date.now())
}

onMounted(async () => {
  const target = ui.editingTask
  if (target) {
    form.title = target.title
    form.description = target.description ?? ''
    form.priority = target.priority
    form.department = target.department ?? ''
    form.recordedAt = toDatetimeInput(target.recordedAt)
  } else {
    form.title = ui.editorSeedTitle
    form.priority = ui.editorSeedPriority ?? 2
    form.recordedAt = nowInputValue()
  }

  await nextTick()
  titleRef.value?.focus()
  titleRef.value?.select()
})

function buildPayload(): CreateTaskInput {
  return {
    title: form.title.trim(),
    description: form.description.trim(),
    priority: form.priority,
    department: form.department.trim() || null,
    recordedAt: fromDatetimeInput(form.recordedAt),
  }
}

async function save() {
  if (saving.value) return
  if (!form.title.trim()) {
    toast.error('请填写事项标题')
    titleRef.value?.focus()
    return
  }

  saving.value = true
  const target = ui.editingTask
  const result = target
    ? await tasks.updateTask(target.id, buildPayload())
    : await tasks.createTask(buildPayload())
  saving.value = false

  if (result) {
    if (target) toast.success('事项已保存')
    ui.closeEditor()
  }
}

function close() {
  ui.closeEditor()
}
</script>

<template>
  <Teleport to="body">
    <div
      class="fixed inset-0 z-40 flex items-start justify-center overflow-y-auto bg-black/35 p-6 pt-[8vh] animate-fade-in"
      @click.self="close"
    >
      <div
        class="w-[560px] max-w-full rounded-xl border border-line bg-panel shadow-[var(--sn-shadow-pop)] animate-pop-in"
        @keydown.ctrl.enter.prevent="save"
      >
        <header class="flex items-center gap-2 border-b border-line px-4 py-2.5">
          <h2 class="text-[13.5px] font-semibold text-ink">
            {{ isEdit ? '编辑事项' : '新建事项' }}
          </h2>
          <span class="text-[11px] text-ink-3">
            {{ isEdit ? '修改会保留创建时间' : '只需标题即可创建' }}
          </span>
          <button
            type="button"
            class="ml-auto grid h-7 w-7 place-items-center rounded text-ink-3 hover:bg-hover hover:text-ink"
            title="关闭 (Esc)"
            @click="close"
          >
            <X :size="15" />
          </button>
        </header>

        <div class="space-y-3 px-4 py-3.5">
          <div>
            <label class="mb-1 block text-[11.5px] font-medium text-ink-2">
              事项标题 <span class="text-p0">*</span>
            </label>
            <input
              ref="titleRef"
              v-model="form.title"
              type="text"
              class="input"
              maxlength="200"
              placeholder="例如：生产服务器异常问题排查"
            />
          </div>

          <div>
            <label class="mb-1 block text-[11.5px] font-medium text-ink-2">优先级</label>
            <div class="flex gap-1.5">
              <button
                v-for="option in PRIORITIES"
                :key="option.value"
                type="button"
                class="chip h-7 flex-1 justify-center border"
                :class="[
                  option.chipClass,
                  form.priority === option.value
                    ? 'border-current'
                    : 'border-transparent opacity-60 hover:opacity-100',
                ]"
                @click="form.priority = option.value"
              >
                {{ option.code }} {{ option.label }}
              </button>
            </div>
          </div>

          <div>
            <label class="mb-1 block text-[11.5px] font-medium text-ink-2">详细描述</label>
            <textarea
              v-model="form.description"
              class="input text-[12.5px]"
              rows="3"
              placeholder="补充背景、影响范围、需要交接的细节（选填）"
            />
          </div>

          <div class="grid grid-cols-2 gap-2">
            <div>
              <label class="mb-1 block text-[11.5px] font-medium text-ink-2">负责部门</label>
              <input
                v-model="form.department"
                class="input"
                type="text"
                maxlength="50"
                placeholder="例如：运维二部（选填）"
              />
            </div>
            <div>
              <label class="mb-1 flex items-center gap-1 text-[11.5px] font-medium text-ink-2">
                记录时间
                <span class="text-[10.5px] font-normal text-ink-3">用于按日期筛选</span>
              </label>
              <div class="flex items-center gap-1.5">
                <input v-model="form.recordedAt" class="input" type="datetime-local" />
                <button
                  type="button"
                  class="btn h-[30px] shrink-0 px-2"
                  title="设为当前时间"
                  @click="form.recordedAt = nowInputValue()"
                >
                  <Clock :size="12" />
                  现在
                </button>
              </div>
            </div>
          </div>
        </div>

        <footer class="flex items-center gap-2 border-t border-line px-4 py-2.5">
          <span class="flex items-center gap-1 text-[10.5px] text-ink-3">
            <CornerDownLeft :size="11" />
            Ctrl+Enter 保存
          </span>
          <div class="ml-auto flex gap-2">
            <button type="button" class="btn h-8" @click="close">取消</button>
            <button
              type="button"
              class="btn btn-primary h-8"
              :disabled="saving || !form.title.trim()"
              @click="save"
            >
              {{ saving ? '保存中…' : isEdit ? '保存修改' : '创建事项' }}
            </button>
          </div>
        </footer>
      </div>
    </div>
  </Teleport>
</template>
