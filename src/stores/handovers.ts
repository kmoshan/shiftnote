import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { save } from '@tauri-apps/plugin-dialog'
import { api, toApiFailure } from '@/services/api'
import { formatDate } from '@/services/format'
import { useConfirmStore } from '@/stores/confirm'
import { useToastStore } from '@/stores/toast'
import type {
  Handover,
  HandoverCandidateFilter,
  HandoverDetail,
  HandoverFilter,
  HandoverFormat,
  HandoverSaveInput,
  Task,
  TaskSnapshot,
  TaskUpdate,
} from '@/types'

const PAGE_SIZE = 50
function freshForm(): HandoverSaveInput {
  const now = new Date()
  const from = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate()
  ).getTime()
  const to = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() + 1
  ).getTime()
  return {
    title: `${formatDate(from)} 交接班`,
    periodStart: from,
    periodEnd: to,
    shiftFrom: null,
    shiftTo: null,
    note: null,
    items: [],
  }
}

export const useHandoverStore = defineStore('handovers', () => {
  const toast = useToastStore()
  const confirm = useConfirmStore()
  const records = ref<Handover[]>([])
  const total = ref(0)
  const page = ref(1)
  const listSearch = ref('')
  const listStatus = ref('')
  const loading = ref(false)
  const editorLoading = ref(false)
  const busy = ref(false)
  const error = ref<string | null>(null)
  const detail = ref<HandoverDetail | null>(null)
  const form = ref<HandoverSaveInput>(freshForm())
  const snapshots = ref<Record<string, TaskSnapshot>>({})
  const availableUpdates = ref<Record<string, TaskUpdate[]>>({})
  const addingIds = ref<Record<string, boolean>>({})
  const candidates = ref<Task[]>([])
  const candidateTotal = ref(0)
  const candidatePage = ref(1)
  const candidateKind = ref<HandoverCandidateFilter['kind']>('all')
  const candidateSearch = ref('')
  const candidatesLoading = ref(false)
  const previewText = ref('')
  const previewFormat = ref<HandoverFormat>('markdown')
  const previewLoading = ref(false)
  const baseline = ref(JSON.stringify(form.value))
  const dirty = computed(() => JSON.stringify(form.value) !== baseline.value)
  const finalized = computed(
    () => detail.value?.handover.status === 'finalized'
  )
  const adding = computed(() => Object.keys(addingIds.value).length > 0)
  const pageCount = computed(() =>
    Math.max(1, Math.ceil(total.value / PAGE_SIZE))
  )
  const candidatePageCount = computed(() =>
    Math.max(1, Math.ceil(candidateTotal.value / PAGE_SIZE))
  )
  let listVersion = 0
  let editorVersion = 0
  let candidateVersion = 0
  let previewVersion = 0
  let liveVersion = 0

  function invalidateLiveReads() {
    ++liveVersion
    ++candidateVersion
    ++previewVersion
    addingIds.value = {}
    candidatesLoading.value = false
    previewLoading.value = false
  }
  function invalidateEditorReads() {
    ++editorVersion
    invalidateLiveReads()
    editorLoading.value = false
  }

  function fail(caught: unknown) {
    error.value = toApiFailure(caught).message
    toast.error(error.value)
  }
  function validate(): boolean {
    if (!form.value.title.trim()) {
      error.value = '请填写交接标题'
      return false
    }
    if (
      !Number.isFinite(form.value.periodStart) ||
      !Number.isFinite(form.value.periodEnd) ||
      form.value.periodStart <= 0 ||
      form.value.periodStart >= form.value.periodEnd
    ) {
      error.value = '班次开始时间必须早于结束时间'
      return false
    }
    error.value = null
    return true
  }
  function accept(next: HandoverDetail) {
    invalidateLiveReads()
    detail.value = next
    const h = next.handover
    form.value = {
      id: h.id,
      title: h.title,
      periodStart: h.periodStart,
      periodEnd: h.periodEnd,
      shiftFrom: h.shiftFrom,
      shiftTo: h.shiftTo,
      note: h.note,
      items: next.items.map((item) => ({
        taskId: item.taskId ?? item.snapshot.task.id,
        section: item.section,
        note: item.note,
        selectedUpdateIds: [...item.selectedUpdateIds],
      })),
    }
    snapshots.value = Object.fromEntries(
      next.items.map((item) => [
        item.taskId ?? item.snapshot.task.id,
        item.snapshot,
      ])
    )
    availableUpdates.value = Object.fromEntries(
      next.items.map((item) => [
        item.taskId ?? item.snapshot.task.id,
        item.snapshot.updates,
      ])
    )
    baseline.value = JSON.stringify(form.value)
    previewText.value = ''
  }
  async function loadList(nextPage = page.value) {
    const version = ++listVersion
    loading.value = true
    const filter: HandoverFilter = {
      search: listSearch.value.trim() || null,
      status: listStatus.value || null,
      limit: PAGE_SIZE,
      offset: (Math.max(1, nextPage) - 1) * PAGE_SIZE,
    }
    try {
      const result = await api.listHandovers(filter)
      if (version !== listVersion) return
      if (result.total > 0 && result.offset >= result.total) {
        await loadList(Math.ceil(result.total / PAGE_SIZE))
        return
      }
      records.value = result.items
      total.value = result.total
      page.value = Math.max(1, nextPage)
      error.value = null
    } catch (caught) {
      if (version === listVersion) fail(caught)
    } finally {
      if (version === listVersion) loading.value = false
    }
  }
  async function open(id: string) {
    const version = ++editorVersion
    invalidateLiveReads()
    editorLoading.value = true
    detail.value = null
    snapshots.value = {}
    availableUpdates.value = {}
    previewText.value = ''
    error.value = null
    try {
      const result = await api.getHandover(id)
      if (version === editorVersion) accept(result)
    } catch (caught) {
      if (version === editorVersion) fail(caught)
    } finally {
      if (version === editorVersion) editorLoading.value = false
    }
  }
  async function startDraft(ids: string[] = []) {
    const version = ++editorVersion
    invalidateLiveReads()
    form.value = freshForm()
    detail.value = null
    snapshots.value = {}
    availableUpdates.value = {}
    addingIds.value = {}
    candidates.value = []
    candidateTotal.value = 0
    candidatePage.value = 1
    previewText.value = ''
    error.value = null
    baseline.value = JSON.stringify(form.value)
    editorLoading.value = true
    const results = await Promise.allSettled(
      [...new Set(ids.filter(Boolean))].map((id) => api.getTask(id))
    )
    if (version !== editorVersion) return
    for (const result of results) {
      if (result.status === 'fulfilled') addResolved(result.value)
      else fail(result.reason)
    }
    editorLoading.value = false
  }
  function addResolved(next: TaskSnapshot) {
    const task = next.task
    if (form.value.items.some((item) => item.taskId === task.id)) return
    if (task.hidden || task.archived || task.deletedAt != null) {
      fail(new Error(`「${task.title}」已隐藏、归档或移入回收站，不能加入交接`))
      return
    }
    snapshots.value = { ...snapshots.value, [task.id]: next }
    availableUpdates.value = {
      ...availableUpdates.value,
      [task.id]: next.updates,
    }
    const ids = next.updates
      .filter(
        (item) =>
          item.kind === 'progress' &&
          item.createdAt >= form.value.periodStart &&
          item.createdAt < form.value.periodEnd
      )
      .map((item) => item.id)
    form.value.items.push({
      taskId: task.id,
      section:
        candidateKind.value === 'new'
          ? 'new'
          : task.status === 'done'
            ? 'handled'
            : 'pending',
      note: null,
      selectedUpdateIds: ids,
    })
    previewText.value = ''
  }
  async function addCandidate(task: Task) {
    if (
      busy.value ||
      finalized.value ||
      addingIds.value[task.id] ||
      form.value.items.some((item) => item.taskId === task.id)
    )
      return
    const version = editorVersion
    const live = liveVersion
    addingIds.value = { ...addingIds.value, [task.id]: true }
    try {
      const next = await api.getTask(task.id)
      if (version === editorVersion && live === liveVersion && !finalized.value)
        addResolved(next)
    } catch (caught) {
      if (version === editorVersion && live === liveVersion) fail(caught)
    } finally {
      if (version === editorVersion && live === liveVersion) {
        const next = { ...addingIds.value }
        delete next[task.id]
        addingIds.value = next
      }
    }
  }
  async function loadItemUpdates(taskId: string) {
    if (finalized.value) return
    const version = editorVersion
    const live = liveVersion
    try {
      const next = await api.getTask(taskId)
      if (
        version !== editorVersion ||
        live !== liveVersion ||
        finalized.value ||
        !form.value.items.some((item) => item.taskId === taskId)
      )
        return
      snapshots.value = { ...snapshots.value, [taskId]: next }
      availableUpdates.value = {
        ...availableUpdates.value,
        [taskId]: next.updates,
      }
    } catch (caught) {
      if (version === editorVersion && live === liveVersion) fail(caught)
    }
  }
  async function loadCandidates(nextPage = 1) {
    if (!validate()) return
    const version = ++candidateVersion
    const editor = editorVersion
    candidatesLoading.value = true
    try {
      const result = await api.handoverCandidates({
        periodStart: form.value.periodStart,
        periodEnd: form.value.periodEnd,
        kind: candidateKind.value,
        search: candidateSearch.value.trim() || null,
        limit: PAGE_SIZE,
        offset: (Math.max(1, nextPage) - 1) * PAGE_SIZE,
      })
      if (version !== candidateVersion || editor !== editorVersion) return
      if (result.total > 0 && result.offset >= result.total) {
        await loadCandidates(Math.ceil(result.total / PAGE_SIZE))
        return
      }
      candidates.value = result.items
      candidateTotal.value = result.total
      candidatePage.value = Math.max(1, nextPage)
    } catch (caught) {
      if (version === candidateVersion && editor === editorVersion) fail(caught)
    } finally {
      if (version === candidateVersion && editor === editorVersion)
        candidatesLoading.value = false
    }
  }
  function removeItem(taskId: string) {
    if (!busy.value && !finalized.value) {
      form.value.items = form.value.items.filter(
        (item) => item.taskId !== taskId
      )
      previewText.value = ''
    }
  }
  function moveItem(index: number, delta: number) {
    if (busy.value || finalized.value) return
    const target = index + delta
    if (target < 0 || target >= form.value.items.length) return
    const items = [...form.value.items]
    ;[items[index], items[target]] = [items[target]!, items[index]!]
    form.value.items = items
  }
  async function saveDraft(): Promise<HandoverDetail | null> {
    if (busy.value || finalized.value || !validate()) return null
    if (adding.value) {
      error.value = '正在读取所选事项，请稍候再保存'
      return null
    }
    const version = editorVersion
    const input = JSON.parse(JSON.stringify(form.value)) as HandoverSaveInput
    busy.value = true
    try {
      const next = await api.saveHandover(input)
      if (version !== editorVersion) return null
      accept(next)
      toast.success('交接草稿已保存')
      return next
    } catch (caught) {
      if (version === editorVersion) fail(caught)
      return null
    } finally {
      busy.value = false
    }
  }
  async function finalize(): Promise<HandoverDetail | null> {
    if (busy.value || finalized.value) return null
    const saved = await saveDraft()
    if (!saved) return null
    const version = editorVersion
    busy.value = true
    try {
      const next = await api.finalizeHandover(saved.handover.id)
      if (version !== editorVersion) return null
      accept(next)
      toast.success('已定稿，交接内容已冻结')
      return next
    } catch (caught) {
      if (version === editorVersion) fail(caught)
      return null
    } finally {
      busy.value = false
    }
  }
  async function preview(format: HandoverFormat = previewFormat.value) {
    const id = detail.value?.handover.id
    if (!id) return
    const version = ++previewVersion
    previewLoading.value = true
    previewFormat.value = format
    try {
      const text = await api.renderHandover(id, format)
      if (version === previewVersion && detail.value?.handover.id === id)
        previewText.value = text
    } catch (caught) {
      if (version === previewVersion) fail(caught)
    } finally {
      if (version === previewVersion) previewLoading.value = false
    }
  }
  async function duplicate(id: string): Promise<HandoverDetail | null> {
    if (busy.value) return null
    busy.value = true
    try {
      const next = await api.duplicateHandover(id)
      toast.success('已复制为新草稿')
      return next
    } catch (caught) {
      fail(caught)
      return null
    } finally {
      busy.value = false
    }
  }
  async function deleteDraft(id: string) {
    if (busy.value) return false
    busy.value = true
    try {
      await api.deleteHandover(id)
      await loadList()
      toast.success('草稿已删除')
      return true
    } catch (caught) {
      fail(caught)
      return false
    } finally {
      busy.value = false
    }
  }
  async function exportFile(format: HandoverFormat) {
    const record = detail.value?.handover
    if (!record || record.status !== 'finalized' || busy.value) return null
    const version = editorVersion
    busy.value = true
    error.value = null
    try {
      const extension = format === 'markdown' ? 'md' : 'txt'
      const path = await save({
        title: '导出明文交接记录（不受口令保护）',
        defaultPath: `${record.title.replace(/[<>:"/\\|?*\u0000-\u001f]/g, '_')}.${extension}`,
        filters: [
          {
            name: format === 'markdown' ? 'Markdown' : '纯文本',
            extensions: [extension],
          },
        ],
      })
      if (!path || version !== editorVersion) return null
      let result
      try {
        result = await api.exportHandover(record.id, format, path, false)
      } catch (caught) {
        if (toApiFailure(caught).code !== 'conflict') throw caught
        if (
          !(await confirm.ask({
            title: '覆盖已有文件？',
            message: `目标文件已存在：${path}`,
            confirmText: '覆盖文件',
            danger: true,
          }))
        )
          return null
        if (version !== editorVersion) return null
        result = await api.exportHandover(record.id, format, path, true)
      }
      toast.success(`已导出：${result.path}`)
      return result
    } catch (caught) {
      if (version === editorVersion) fail(caught)
      return null
    } finally {
      busy.value = false
    }
  }
  function resetAfterRestore() {
    ++listVersion
    ++editorVersion
    ++candidateVersion
    ++previewVersion
    invalidateEditorReads()
    records.value = []
    total.value = 0
    detail.value = null
    form.value = freshForm()
    snapshots.value = {}
    availableUpdates.value = {}
    candidates.value = []
    candidateTotal.value = 0
    previewText.value = ''
    baseline.value = JSON.stringify(form.value)
    error.value = null
    loading.value = false
    editorLoading.value = false
    candidatesLoading.value = false
    previewLoading.value = false
  }
  return {
    records,
    total,
    page,
    listSearch,
    listStatus,
    loading,
    editorLoading,
    busy,
    error,
    detail,
    form,
    snapshots,
    availableUpdates,
    addingIds,
    adding,
    candidates,
    candidateTotal,
    candidatePage,
    candidateKind,
    candidateSearch,
    candidatesLoading,
    previewText,
    previewFormat,
    previewLoading,
    dirty,
    finalized,
    pageCount,
    candidatePageCount,
    loadList,
    open,
    startDraft,
    loadCandidates,
    addCandidate,
    loadItemUpdates,
    removeItem,
    moveItem,
    saveDraft,
    finalize,
    preview,
    duplicate,
    deleteDraft,
    exportFile,
    resetAfterRestore,
    invalidateEditorReads,
  }
})
