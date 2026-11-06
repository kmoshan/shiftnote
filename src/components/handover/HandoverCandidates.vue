<script setup lang="ts">
import { useHandoverStore } from '@/stores/handovers'
import { formatDateTime } from '@/services/format'
const handovers = useHandoverStore()
</script>
<template>
  <aside class="flex min-h-0 flex-col rounded-lg border border-line bg-panel">
    <header class="border-b border-line p-3">
      <h3 class="text-[12.5px] font-semibold">选择交接事项</h3>
      <p class="mt-1 text-[11px] text-ink-3">
        排除隐藏、归档和回收站事项。当前待处理不受班次时间限制。
      </p>
    </header>
    <form
      class="space-y-2 border-b border-line p-3"
      @submit.prevent="handovers.loadCandidates(1)"
    >
      <select
        v-model="handovers.candidateKind"
        class="input w-full"
        aria-label="候选范围"
        :disabled="handovers.busy"
        @change="handovers.loadCandidates(1)"
      >
        <option value="all">本班新增 / 完成 / 有进展及当前待处理</option>
        <option value="new">本班新增（记录时间）</option>
        <option value="handled">本班完成（完成时间）</option>
        <option value="pending">当前待处理</option>
        <option value="progress">本班有进展</option>
      </select>
      <div class="flex gap-2">
        <input
          v-model="handovers.candidateSearch"
          class="input min-w-0 flex-1"
          placeholder="搜索候选事项"
          aria-label="搜索候选事项"
          :disabled="handovers.busy"
        /><button
          class="btn"
          :disabled="handovers.busy || handovers.candidatesLoading"
        >
          查询
        </button>
      </div>
    </form>
    <div class="max-h-[430px] min-h-24 space-y-1 overflow-y-auto p-2">
      <p
        v-if="handovers.candidatesLoading"
        class="p-3 text-[11.5px] text-ink-3"
      >
        正在查询…
      </p>
      <p
        v-else-if="!handovers.candidates.length"
        class="p-3 text-[11.5px] text-ink-3"
      >
        当前范围没有候选事项，可调整班次或候选范围。
      </p>
      <button
        v-for="task in handovers.candidates"
        :key="task.id"
        type="button"
        class="flex w-full items-start gap-2 rounded-md border border-transparent px-2 py-2 text-left hover:border-line hover:bg-hover disabled:opacity-50"
        :disabled="
          handovers.busy ||
          handovers.addingIds[task.id] ||
          handovers.form.items.some((item) => item.taskId === task.id)
        "
        @click="handovers.addCandidate(task)"
      >
        <span class="mt-0.5 text-[11px] font-semibold text-accent">{{
          handovers.form.items.some((item) => item.taskId === task.id)
            ? '已选'
            : '＋'
        }}</span
        ><span class="min-w-0 flex-1"
          ><span class="block break-words text-[12px] text-ink">{{
            task.title
          }}</span
          ><span class="mt-0.5 block text-[10.5px] text-ink-3"
            >P{{ task.priority }} ·
            {{ task.status === 'done' ? '已完成' : '待处理' }} ·
            {{ task.department || '无部门' }}</span
          ><span class="block text-[10.5px] text-ink-3"
            >记录于 {{ formatDateTime(task.recordedAt) }}</span
          ></span
        >
      </button>
    </div>
    <footer
      class="flex flex-wrap items-center gap-2 border-t border-line p-2 text-[10.5px] text-ink-3"
    >
      <span class="flex-1"
        >共 {{ handovers.candidateTotal }} 条 · {{ handovers.candidatePage }}/{{
          handovers.candidatePageCount
        }}
        页</span
      ><button
        class="btn"
        :disabled="
          handovers.busy ||
          handovers.candidatesLoading ||
          handovers.candidatePage <= 1
        "
        @click="handovers.loadCandidates(handovers.candidatePage - 1)"
      >
        上一页</button
      ><button
        class="btn"
        :disabled="
          handovers.busy ||
          handovers.candidatesLoading ||
          handovers.candidatePage >= handovers.candidatePageCount
        "
        @click="handovers.loadCandidates(handovers.candidatePage + 1)"
      >
        下一页
      </button>
    </footer>
  </aside>
</template>
