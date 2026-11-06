<script setup lang="ts">
import { computed, ref } from 'vue'
import { CornerDownLeft, Plus } from '@lucide/vue'

import { useTaskStore } from '@/stores/tasks'
import { useUiStore } from '@/stores/ui'
import { priorityMeta } from '@/types/priority'

const tasks = useTaskStore()
const ui = useUiStore()

const title = ref('')
const priority = ref(2)
const inputRef = ref<HTMLInputElement | null>(null)
const saving = ref(false)

const meta = computed(() => priorityMeta(priority.value))

function cyclePriority() {
  priority.value = (priority.value + 1) % 4
}

async function submit() {
  const value = title.value.trim()
  if (!value || saving.value) return

  saving.value = true
  const created = await tasks.createTask(
    { title: value, priority: priority.value },
    { select: false }
  )
  saving.value = false

  if (created) {
    title.value = ''
    inputRef.value?.focus()
  }
}

function expandToEditor() {
  ui.openCreate({ title: title.value.trim(), priority: priority.value })
  title.value = ''
}
</script>

<template>
  <div class="px-3 pb-1.5">
    <div
      class="flex items-center gap-2 rounded-lg border border-line bg-panel px-2.5 py-1.5 transition-colors focus-within:border-accent"
    >
      <Plus :size="15" class="shrink-0 text-ink-3" />

      <input
        ref="inputRef"
        v-model="title"
        type="text"
        class="h-6 min-w-0 flex-1 bg-transparent text-[13px] text-ink outline-none placeholder:text-ink-3"
        placeholder="快速添加事项：输入标题后回车保存，Shift+回车填写详情"
        @keydown.enter.exact.prevent="submit"
        @keydown.enter.shift.prevent="expandToEditor"
      />

      <button
        type="button"
        class="chip shrink-0"
        :class="meta.chipClass"
        :title="`当前优先级 ${meta.code} ${meta.label}，点击切换`"
        @click="cyclePriority"
      >
        {{ meta.code }} {{ meta.label }}
      </button>

      <span class="hidden shrink-0 items-center gap-1 text-[10.5px] text-ink-3 xl:flex">
        <CornerDownLeft :size="11" />
        回车保存
      </span>
    </div>
  </div>
</template>
