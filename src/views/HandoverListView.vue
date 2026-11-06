<script setup lang="ts">
import { onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { Plus, ClipboardList, Copy, Trash2, Search } from '@lucide/vue'
import { useHandoverStore } from '@/stores/handovers'
import { useConfirmStore } from '@/stores/confirm'
import { formatDateTime } from '@/services/format'

const handovers = useHandoverStore()
const confirm = useConfirmStore()
const router = useRouter()
onMounted(() => handovers.loadList(1))
async function duplicate(id: string) {
  const next = await handovers.duplicate(id)
  if (next) await router.push(`/handovers/${next.handover.id}`)
}
async function remove(id: string, title: string) {
  if (
    await confirm.ask({
      title: '删除交接草稿？',
      message: `「${title}」将被删除，事项和进展不受影响。`,
      confirmText: '删除草稿',
      danger: true,
    })
  )
    await handovers.deleteDraft(id)
}
</script>

<template>
  <section class="flex min-h-0 flex-1 flex-col">
    <header
      class="flex flex-wrap items-center gap-3 border-b border-line px-5 py-4"
    >
      <div class="min-w-0 flex-1">
        <h2 class="text-[15px] font-semibold">
          交接记录
          <span class="ml-2 text-[11px] font-normal text-ink-3"
            >共 {{ handovers.total }} 份</span
          >
        </h2>
        <p class="mt-1 text-[11.5px] text-ink-3">
          草稿关联当前事项；定稿后保留当班内容，可导出 Markdown 或文本。
        </p>
      </div>
      <router-link to="/handovers/new" class="btn btn-primary"
        ><Plus :size="14" /> 新建交接</router-link
      >
    </header>
    <form
      class="flex flex-wrap gap-2 px-5 py-3"
      @submit.prevent="handovers.loadList(1)"
    >
      <label class="relative min-w-40 flex-1"
        ><Search :size="14" class="absolute left-2 top-2 text-ink-3" /><input
          v-model="handovers.listSearch"
          class="input w-full pl-7"
          aria-label="搜索交接记录"
          placeholder="搜索标题、交接人或备注"
      /></label>
      <select
        v-model="handovers.listStatus"
        class="input"
        aria-label="交接状态"
        @change="handovers.loadList(1)"
      >
        <option value="">全部状态</option>
        <option value="draft">草稿</option>
        <option value="finalized">已定稿</option>
      </select>
      <button type="submit" class="btn" :disabled="handovers.loading">
        搜索
      </button>
    </form>
    <p
      v-if="handovers.error"
      role="alert"
      class="mx-5 mb-2 rounded bg-p0-soft p-2 text-[12px] text-p0"
    >
      {{ handovers.error }}
      <button class="underline" @click="handovers.loadList()">重试</button>
    </p>
    <div class="min-h-0 flex-1 space-y-2 overflow-y-auto px-5 pb-4">
      <p
        v-if="handovers.loading"
        class="py-8 text-center text-[12px] text-ink-3"
      >
        正在读取交接记录…
      </p>
      <div
        v-else-if="!handovers.records.length"
        class="flex flex-col items-center gap-2 py-16 text-ink-3"
      >
        <ClipboardList :size="30" />
        <p class="text-[13px]">没有匹配的交接记录</p>
        <p class="text-[11.5px]">新建交接，选择当班事项并确认交接内容。</p>
      </div>
      <article
        v-for="record in handovers.records"
        :key="record.id"
        class="rounded-lg border border-line bg-panel px-4 py-3"
      >
        <div class="flex items-start gap-3">
          <router-link :to="`/handovers/${record.id}`" class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <h3 class="truncate text-[13px] font-semibold text-ink">
                {{ record.title }}
              </h3>
              <span
                class="shrink-0 rounded bg-panel-2 px-1.5 py-0.5 text-[10.5px]"
                :class="
                  record.status === 'finalized' ? 'text-success' : 'text-ink-3'
                "
                >{{ record.status === 'finalized' ? '已定稿' : '草稿' }}</span
              >
            </div>
            <p class="mt-1 text-[11px] text-ink-3">
              {{ formatDateTime(record.periodStart) }} →
              {{ formatDateTime(record.periodEnd) }}
            </p>
            <p class="mt-1 text-[11.5px] text-ink-2">
              {{ record.shiftFrom || '交班人未填写' }} →
              {{ record.shiftTo || '接班人未填写' }}
            </p>
            <p
              v-if="record.note"
              class="mt-1 truncate text-[11.5px] text-ink-3"
            >
              {{ record.note }}
            </p>
          </router-link>
          <button
            type="button"
            class="btn btn-ghost"
            :disabled="handovers.busy"
            title="复制为新草稿"
            @click="duplicate(record.id)"
          >
            <Copy :size="13" /> 复制
          </button>
          <button
            v-if="record.status === 'draft'"
            type="button"
            class="btn btn-ghost text-p0"
            :disabled="handovers.busy"
            title="删除草稿"
            @click="remove(record.id, record.title)"
          >
            <Trash2 :size="13" />
          </button>
        </div>
      </article>
    </div>
    <footer
      class="flex items-center justify-between border-t border-line px-5 py-3 text-[11.5px] text-ink-3"
    >
      <span
        >第 {{ handovers.page }} / {{ handovers.pageCount }} 页 · 每页 50
        份</span
      >
      <div class="flex gap-2">
        <button
          class="btn"
          :disabled="handovers.loading || handovers.page <= 1"
          @click="handovers.loadList(handovers.page - 1)"
        >
          上一页</button
        ><button
          class="btn"
          :disabled="handovers.loading || handovers.page >= handovers.pageCount"
          @click="handovers.loadList(handovers.page + 1)"
        >
          下一页
        </button>
      </div>
    </footer>
  </section>
</template>
