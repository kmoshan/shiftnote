<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useTaskStore } from '@/stores/tasks'
import { PRIORITIES } from '@/types/priority'
const tasks = useTaskStore()
const router = useRouter()
const priority = ref('')
const selected = computed(() =>
  tasks.tasks.filter((task) => tasks.selectedIds.includes(task.id))
)
const inTrash = computed(
  () =>
    selected.value.length > 0 &&
    selected.value.every((task) => task.deletedAt !== null)
)
const canArchive = computed(
  () =>
    selected.value.length > 0 &&
    selected.value.every((task) => task.status === 'done' && !task.archived)
)
const hasArchived = computed(() => selected.value.some((task) => task.archived))
async function setPriority() {
  if (priority.value === '') return
  await tasks.applyBatch('priority', Number(priority.value))
  priority.value = ''
}
function startHandover() {
  if (tasks.batchBusy || tasks.selectedIds.length === 0) return
  void router.push({
    path: '/handovers/new',
    query: { taskIds: tasks.selectedIds.join(',') },
  })
}
</script>
<template>
  <div class="mx-3.5 mb-2 rounded-md border border-line bg-panel-2 px-2.5 py-2">
    <fieldset
      class="flex flex-wrap items-center gap-1.5 text-[11.5px]"
      :disabled="tasks.batchBusy || tasks.loading"
    >
      <label class="mr-1 flex items-center gap-1.5 text-ink-2"
        ><input
          type="checkbox"
          class="accent-accent"
          :checked="tasks.allPageSelected"
          :disabled="tasks.tasks.length === 0"
          @change="tasks.selectCurrentPage()"
        />全选当前页</label
      >
      <span class="tabular-nums text-ink-3"
        >已选 {{ tasks.selectedIds.length }} 条</span
      >
      <template v-if="tasks.selectedIds.length">
        <button
          type="button"
          class="btn h-6 px-2 text-[11px]"
          @click="tasks.clearSelection()"
        >
          取消选择
        </button>
        <template v-if="inTrash">
          <button
            type="button"
            class="btn h-6 px-2 text-[11px]"
            @click="tasks.applyBatch('restore')"
          >
            批量恢复
          </button>
          <button
            type="button"
            class="btn btn-danger h-6 px-2 text-[11px]"
            @click="tasks.applyBatch('deletePermanent')"
          >
            永久删除
          </button>
        </template>
        <template v-else>
          <button
            type="button"
            class="btn h-6 px-2 text-[11px]"
            @click="tasks.applyBatch('complete')"
          >
            完成
          </button>
          <button
            type="button"
            class="btn h-6 px-2 text-[11px]"
            @click="tasks.applyBatch('reopen')"
          >
            取消完成
          </button>
          <button
            type="button"
            class="btn h-6 px-2 text-[11px]"
            @click="tasks.applyBatch('hide')"
          >
            隐藏
          </button>
          <button
            type="button"
            class="btn h-6 px-2 text-[11px]"
            @click="tasks.applyBatch('unhide')"
          >
            取消隐藏
          </button>
          <select
            v-model="priority"
            aria-label="批量修改优先级"
            class="input h-6 w-[105px] px-1 text-[11px]"
            @change="setPriority"
          >
            <option value="">修改优先级</option>
            <option
              v-for="option in PRIORITIES"
              :key="option.value"
              :value="String(option.value)"
            >
              {{ option.code }} {{ option.label }}
            </option>
          </select>
          <button
            v-if="canArchive"
            type="button"
            class="btn h-6 px-2 text-[11px]"
            @click="tasks.applyBatch('archive')"
          >
            归档
          </button>
          <button
            v-if="hasArchived"
            type="button"
            class="btn h-6 px-2 text-[11px]"
            @click="tasks.applyBatch('unarchive')"
          >
            取消归档
          </button>
          <button
            type="button"
            class="btn btn-danger h-6 px-2 text-[11px]"
            @click="tasks.applyBatch('trash')"
          >
            移入回收站
          </button>
          <button
            type="button"
            class="btn h-6 px-2 text-[11px]"
            @click="startHandover"
          >
            从已选事项发起交接
          </button>
        </template>
      </template>
      <span v-if="tasks.batchBusy" role="status" class="text-ink-3"
        >正在提交，已锁定 {{ tasks.selectedIds.length }} 条事项…</span
      >
    </fieldset>
    <p
      v-if="tasks.batchError"
      role="alert"
      class="mt-1.5 break-words text-[11.5px] text-p0"
    >
      {{ tasks.batchError }} · 选择已保留，可调整后重试。
    </p>
  </div>
</template>
