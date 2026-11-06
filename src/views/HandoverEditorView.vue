<script setup lang="ts">
import { computed, watch, onUnmounted } from 'vue'
import {
  onBeforeRouteLeave,
  onBeforeRouteUpdate,
  useRoute,
  useRouter,
} from 'vue-router'
import { ArrowLeft, Save, Lock, Copy, Download, Eye } from '@lucide/vue'
import HandoverCandidates from '@/components/handover/HandoverCandidates.vue'
import HandoverItems from '@/components/handover/HandoverItems.vue'
import { useHandoverStore } from '@/stores/handovers'
import { useConfirmStore } from '@/stores/confirm'
import type { HandoverFormat } from '@/types'
import {
  toDatetimeInput,
  fromDatetimeInput,
  formatDateTime,
} from '@/services/format'

const handovers = useHandoverStore()
const confirm = useConfirmStore()
const route = useRoute()
const router = useRouter()
const fromInput = computed({
  get: () => toDatetimeInput(handovers.form.periodStart),
  set: (value) => {
    handovers.form.periodStart = fromDatetimeInput(value) ?? 0
  },
})
const toInput = computed({
  get: () => toDatetimeInput(handovers.form.periodEnd),
  set: (value) => {
    handovers.form.periodEnd = fromDatetimeInput(value) ?? 0
  },
})
const missingRecord = computed(
  () =>
    typeof route.params.id === 'string' &&
    handovers.detail?.handover.id !== route.params.id
)
const intervalInvalid = computed(
  () =>
    handovers.form.periodStart <= 0 ||
    handovers.form.periodStart >= handovers.form.periodEnd
)
let savedRouteId: string | null = null

async function load() {
  const id = typeof route.params.id === 'string' ? route.params.id : null
  if (id) {
    if (savedRouteId === id) savedRouteId = null
    else await handovers.open(id)
  } else {
    const seed =
      typeof route.query.taskIds === 'string'
        ? route.query.taskIds.split(',').slice(0, 1000)
        : []
    await handovers.startDraft(seed)
  }
  if (!handovers.finalized && !missingRecord.value)
    await handovers.loadCandidates(1)
}
watch(() => route.fullPath, load, { immediate: true })
async function allowLeave() {
  if (handovers.busy) return false
  if (!handovers.dirty || handovers.finalized) return true
  return confirm.ask({
    title: '离开未保存的交接？',
    message: '当前交接有未保存内容。选择继续编辑后，可先保存草稿。',
    confirmText: '放弃修改并离开',
    cancelText: '继续编辑',
    danger: true,
  })
}
onBeforeRouteLeave(allowLeave)
onUnmounted(() => handovers.invalidateEditorReads())
onBeforeRouteUpdate((to) => {
  if (savedRouteId && to.params.id === savedRouteId) return true
  return allowLeave()
})
async function saveDraft() {
  const next = await handovers.saveDraft()
  if (next && route.params.id !== next.handover.id) {
    savedRouteId = next.handover.id
    await router.replace(`/handovers/${next.handover.id}`)
  }
  return next
}
async function preview() {
  if (!handovers.finalized && (handovers.dirty || !handovers.detail)) {
    if (!(await saveDraft())) return
  }
  await handovers.preview()
}
function showPreview(format: HandoverFormat) {
  handovers.previewFormat = format
  void preview()
}
async function finalize() {
  if (
    !(await confirm.ask({
      title: '定稿并冻结交接内容？',
      message: `将保存并冻结当前 ${handovers.form.items.length} 条交接事项及选定进展。后续修改或删除原事项不会改变定稿；需要调整时可复制为新草稿。`,
      confirmText: '保存并定稿',
    }))
  )
    return
  const next = await handovers.finalize()
  if (!next) return
  if (route.params.id !== next.handover.id) {
    savedRouteId = next.handover.id
    await router.replace(`/handovers/${next.handover.id}`)
  }
  await handovers.preview()
}
async function duplicate() {
  if (!handovers.detail) return
  const next = await handovers.duplicate(handovers.detail.handover.id)
  if (next) await router.push(`/handovers/${next.handover.id}`)
}
</script>

<template>
  <section class="flex min-h-0 min-w-0 flex-1 flex-col">
    <header
      class="flex flex-wrap items-center gap-2 border-b border-line px-4 py-3"
    >
      <router-link to="/handovers" class="btn btn-ghost" title="返回交接记录"
        ><ArrowLeft :size="14"
      /></router-link>
      <h2 class="mr-auto text-[14px] font-semibold">
        {{
          handovers.finalized
            ? '交接历史'
            : handovers.detail
              ? '编辑交接草稿'
              : '新建交接'
        }}
        <span class="ml-2 text-[10.5px] font-normal text-ink-3">{{
          handovers.finalized
            ? '已定稿'
            : handovers.dirty
              ? '有未保存修改'
              : '草稿'
        }}</span>
      </h2>
      <template v-if="!handovers.editorLoading && !missingRecord">
        <button
          v-if="!handovers.finalized"
          class="btn"
          :disabled="handovers.busy || handovers.adding || intervalInvalid"
          @click="saveDraft"
        >
          <Save :size="13" /> 保存草稿
        </button>
        <button
          class="btn"
          :disabled="
            handovers.busy ||
            handovers.adding ||
            handovers.previewLoading ||
            intervalInvalid
          "
          @click="preview"
        >
          <Eye :size="13" /> {{ handovers.previewLoading ? '生成中…' : '预览' }}
        </button>
        <button
          v-if="!handovers.finalized"
          class="btn btn-primary"
          :disabled="handovers.busy || handovers.adding || intervalInvalid"
          @click="finalize"
        >
          <Lock :size="13" /> 定稿并冻结
        </button>
        <template v-else
          ><button class="btn" :disabled="handovers.busy" @click="duplicate">
            <Copy :size="13" /> 复制为新草稿</button
          ><button
            class="btn"
            title="导出未加密 Markdown 文件"
            :disabled="handovers.busy"
            @click="handovers.exportFile('markdown')"
          >
            <Download :size="13" /> Markdown</button
          ><button
            class="btn btn-primary"
            title="导出未加密 TXT 文件"
            :disabled="handovers.busy"
            @click="handovers.exportFile('txt')"
          >
            <Download :size="13" /> TXT
          </button></template
        >
      </template>
    </header>
    <div
      v-if="handovers.editorLoading"
      class="p-8 text-center text-[12px] text-ink-3"
    >
      正在读取交接内容…
    </div>
    <div v-else-if="missingRecord" class="p-8 text-center text-[12px] text-p0">
      {{ handovers.error || '无法读取交接记录' }}
      <button class="underline" @click="load">重试</button>
    </div>
    <div v-else class="min-h-0 flex-1 overflow-y-auto p-4">
      <p
        v-if="handovers.error"
        role="alert"
        class="mb-3 whitespace-pre-wrap rounded bg-p0-soft p-3 text-[12px] text-p0"
      >
        {{ handovers.error }}
      </p>
      <p
        v-if="handovers.finalized"
        class="mb-3 rounded border border-line bg-panel-2 p-3 text-[11.5px] text-ink-2"
      >
        已于
        {{ formatDateTime(handovers.detail!.handover.finalizedAt!) }}
        定稿，以下内容和导出文件使用同一份冻结快照。
        <span class="mt-1 block">Markdown / TXT 导出为未加密文件，不受数据口令保护。任何持有文件的人都能阅读，请仅保存或分享给需要的人。</span>
      </p>
      <div
        class="grid items-start gap-4"
        :class="
          handovers.finalized
            ? 'grid-cols-1'
            : 'xl:grid-cols-[minmax(0,1fr)_minmax(280px,340px)]'
        "
      >
        <div class="min-w-0 space-y-4">
          <fieldset
            class="space-y-3 rounded-lg border border-line bg-panel p-4 disabled:opacity-80"
            :disabled="handovers.busy || handovers.finalized"
          >
            <label class="block text-[11.5px] text-ink-3"
              >交接标题<input
                v-model="handovers.form.title"
                class="input mt-1 w-full"
                maxlength="200"
                placeholder="例如：11 月 6 日夜班交接"
            /></label>
            <div class="grid gap-3 sm:grid-cols-2">
              <label class="block min-w-0 text-[11.5px] text-ink-3"
                >班次开始<input
                  v-model="fromInput"
                  type="datetime-local"
                  class="input mt-1 w-full min-w-0"
                  @change="
                    !intervalInvalid && handovers.loadCandidates(1)
                  " /></label
              ><label class="block min-w-0 text-[11.5px] text-ink-3"
                >班次结束<input
                  v-model="toInput"
                  type="datetime-local"
                  class="input mt-1 w-full min-w-0"
                  @change="!intervalInvalid && handovers.loadCandidates(1)"
              /></label>
            </div>
            <p v-if="intervalInvalid" role="alert" class="text-[11px] text-p0">
              开始时间必须早于结束时间。跨午夜班次请将结束日期设为次日。
            </p>
            <p v-else class="text-[10.5px] text-ink-3">
              按本地时间显示，支持跨午夜；包含开始时刻，不包含结束时刻。
            </p>
            <div class="grid grid-cols-2 gap-3">
              <label class="block text-[11.5px] text-ink-3"
                >交班人<input
                  v-model="handovers.form.shiftFrom"
                  class="input mt-1 w-full"
                  placeholder="可选" /></label
              ><label class="block text-[11.5px] text-ink-3"
                >接班人<input
                  v-model="handovers.form.shiftTo"
                  class="input mt-1 w-full"
                  placeholder="可选"
              /></label>
            </div>
            <label class="block text-[11.5px] text-ink-3"
              >整体备注<textarea
                v-model="handovers.form.note"
                class="input mt-1 w-full resize-y"
                rows="3"
                placeholder="当班概况、注意事项与交接说明"
              />
            </label>
          </fieldset>
          <div>
            <h3 class="mb-2 text-[12.5px] font-semibold">
              交接事项
              <span class="text-[11px] font-normal text-ink-3"
                >{{ handovers.form.items.length }} 条 ·
                每个事项一个主分区，P0/P1 保留为重点标签</span
              >
            </h3>
            <HandoverItems />
          </div>
        </div>
        <HandoverCandidates v-if="!handovers.finalized" />
      </div>
      <div
        v-if="handovers.previewText"
        class="mt-4 rounded-lg border border-line bg-panel"
      >
        <header
          class="flex flex-wrap items-center gap-2 border-b border-line p-3"
        >
          <h3 class="mr-auto text-[12.5px] font-semibold">
            {{ handovers.finalized ? '定稿导出预览' : '草稿导出预览' }}
          </h3>
          <span v-if="handovers.dirty" class="text-[11px] text-p0"
            >内容已修改，请重新预览</span
          ><button
            class="btn"
            :disabled="handovers.previewLoading || handovers.busy"
            @click="showPreview('markdown')"
          >
            Markdown</button
          ><button
            class="btn"
            :disabled="handovers.previewLoading || handovers.busy"
            @click="showPreview('txt')"
          >
            TXT
          </button>
        </header>
        <pre
          class="overflow-x-auto whitespace-pre-wrap break-words p-4 font-mono text-[11.5px] leading-relaxed text-ink-2"
          >{{ handovers.previewText }}</pre>
      </div>
    </div>
  </section>
</template>
