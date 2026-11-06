<script setup lang="ts">
import { useRouter, isNavigationFailure } from 'vue-router'
import { useHandoverStore } from '@/stores/handovers'
import { useTaskStore } from '@/stores/tasks'
import { formatDateTime } from '@/services/format'
const handovers = useHandoverStore()
const tasks = useTaskStore()
const router = useRouter()
const sections = [
  { value: 'new', label: '本班新增' },
  { value: 'handled', label: '本班已处理' },
  { value: 'pending', label: '待处理' },
  { value: 'follow_up', label: '需要跟进' },
  { value: 'focus', label: '重点关注' },
]
function updates(taskId: string) {
  return (handovers.availableUpdates[taskId] ?? []).filter(
    (update) =>
      update.kind === 'progress' &&
      update.createdAt >= handovers.form.periodStart &&
      update.createdAt < handovers.form.periodEnd
  )
}
function invalidSelections(taskId: string, selected: string[]) {
  const valid = new Set(updates(taskId).map((update) => update.id))
  return selected.filter((id) => !valid.has(id))
}
function unavailable(taskId: string) {
  return Boolean(
    handovers.detail?.items.some(
      (item) => item.snapshot.task.id === taskId && !item.taskId
    )
  )
}
async function jump(taskId: string) {
  const result = await router.push('/')
  if (!isNavigationFailure(result)) await tasks.selectTask(taskId)
}
</script>
<template>
  <div class="space-y-2">
    <p
      v-if="!handovers.form.items.length"
      class="rounded-lg border border-dashed border-line py-8 text-center text-[12px] text-ink-3"
    >
      尚未加入事项，从候选列表中选择。也可以保存空交接并填写整体备注。
    </p>
    <article
      v-for="(item, index) in handovers.form.items"
      :key="item.taskId"
      class="space-y-2 rounded-lg border border-line bg-panel p-3"
    >
      <div class="flex items-start gap-2">
        <span class="mt-0.5 text-[11px] text-ink-3">{{ index + 1 }}.</span>
        <div class="min-w-0 flex-1">
          <h4 class="break-words text-[12.5px] font-semibold">
            {{ handovers.snapshots[item.taskId]?.task.title ?? item.taskId }}
          </h4>
          <p class="mt-1 text-[10.5px] text-ink-3">
            P{{ handovers.snapshots[item.taskId]?.task.priority ?? '—' }} ·
            {{
              handovers.snapshots[item.taskId]?.task.department || '无部门'
            }}
            ·
            {{
              handovers.snapshots[item.taskId]?.task.status === 'done'
                ? '已完成'
                : '待处理'
            }}
          </p>
        </div>
        <button
          v-if="!unavailable(item.taskId)"
          class="text-[11px] text-accent hover:underline"
          type="button"
          :disabled="handovers.busy"
          @click="jump(item.taskId)"
        >
          查看事项
        </button>
      </div>
      <p
        v-if="unavailable(item.taskId)"
        class="rounded bg-p0-soft p-2 text-[11px] text-p0"
      >
        原事项已永久删除，保留上次保存内容。{{
          handovers.finalized
            ? '定稿快照不受影响。'
            : '请移除该条目后再保存或定稿。'
        }}
      </p>
      <div v-if="!handovers.finalized" class="flex items-center gap-2">
        <label class="text-[11px] text-ink-3">分区</label
        ><select
          v-model="item.section"
          class="input min-w-0 flex-1"
          :disabled="handovers.busy"
          aria-label="交接分区"
        >
          <option
            v-for="section in sections"
            :key="section.value"
            :value="section.value"
          >
            {{ section.label }}
          </option></select
        ><button
          class="btn"
          title="上移条目"
          :disabled="handovers.busy || index === 0"
          @click="handovers.moveItem(index, -1)"
        >
          ↑</button
        ><button
          class="btn"
          title="下移条目"
          :disabled="
            handovers.busy || index === handovers.form.items.length - 1
          "
          @click="handovers.moveItem(index, 1)"
        >
          ↓</button
        ><button
          class="btn text-p0"
          :disabled="handovers.busy"
          @click="handovers.removeItem(item.taskId)"
        >
          移除
        </button>
      </div>
      <p v-else class="text-[11px] font-medium text-accent">
        {{ sections.find((section) => section.value === item.section)?.label }}
      </p>
      <p
        v-if="handovers.snapshots[item.taskId]?.task.description"
        class="whitespace-pre-wrap break-words text-[11.5px] text-ink-2"
      >
        {{ handovers.snapshots[item.taskId]?.task.description }}
      </p>
      <details
        @toggle="
          ($event.target as HTMLDetailsElement).open &&
          handovers.loadItemUpdates(item.taskId)
        "
      >
        <summary class="cursor-pointer text-[11.5px] text-ink-3">
          {{ handovers.finalized ? '已冻结进展' : '选择交接进展' }} ·
          {{ item.selectedUpdateIds.length }} 条
        </summary>
        <div class="mt-2 space-y-2">
          <template v-if="handovers.finalized"
            ><p
              v-for="update in handovers.snapshots[item.taskId]?.updates ?? []"
              :key="update.id"
              class="whitespace-pre-wrap break-words border-l-2 border-line pl-2 text-[11.5px]"
            >
              <span class="mb-1 block text-[10.5px] text-ink-3"
                >{{ formatDateTime(update.createdAt) }} ·
                {{ update.author || '未署名' }}</span
              >{{ update.content }}
            </p></template
          >
          <template v-else
            ><p class="text-[10.5px] text-ink-3">
              仅显示班次时间内的业务进展，可逐条选入。
            </p>
            <p
              v-if="
                invalidSelections(item.taskId, item.selectedUpdateIds).length
              "
              class="text-[11px] text-p0"
            >
              有
              {{
                invalidSelections(item.taskId, item.selectedUpdateIds).length
              }}
              条已选进展被删除或已超出当前班次。<button
                type="button"
                class="underline"
                :disabled="handovers.busy"
                @click="
                  item.selectedUpdateIds = item.selectedUpdateIds.filter((id) =>
                    updates(item.taskId).some((update) => update.id === id)
                  )
                "
              >
                移除失效选择
              </button>
            </p>
            <label
              v-for="update in updates(item.taskId)"
              :key="update.id"
              class="flex items-start gap-2 rounded bg-panel-2 p-2"
              ><input
                v-model="item.selectedUpdateIds"
                :value="update.id"
                type="checkbox"
                class="mt-1"
                :disabled="handovers.busy"
              /><span
                class="min-w-0 flex-1 whitespace-pre-wrap break-words text-[11.5px]"
                ><span class="mb-1 block text-[10.5px] text-ink-3"
                  >{{ formatDateTime(update.createdAt) }} ·
                  {{ update.author || '未署名' }}</span
                >{{ update.content }}</span
              ></label
            >
            <p
              v-if="!updates(item.taskId).length"
              class="text-[11px] text-ink-3"
            >
              班次内没有可选进展。
            </p></template
          >
        </div>
      </details>
      <label class="block text-[11px] text-ink-3"
        >交接备注<textarea
          v-model="item.note"
          rows="2"
          class="input mt-1 w-full resize-y text-[12px]"
          :disabled="handovers.busy || handovers.finalized"
          placeholder="补充处理结果或下一班需跟进的内容"
        />
      </label>
    </article>
  </div>
</template>
