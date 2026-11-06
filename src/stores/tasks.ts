import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { api, toApiFailure } from '@/services/api'
import { localDayBounds } from '@/services/format'
import { useConfirmStore } from '@/stores/confirm'
import { useToastStore } from '@/stores/toast'
import { useVaultStore } from '@/stores/vault'
import type {
  Counts,
  CreateTaskInput,
  SortKey,
  Task,
  TaskBatchAction,
  TaskDetail,
  TaskFilter,
  TaskUpdate,
  ViewKey,
} from '@/types'

const EMPTY_COUNTS: Counts = {
  all: 0,
  today: 0,
  open: 0,
  focus: 0,
  done: 0,
  hidden: 0,
  archived: 0,
  trash: 0,
}
interface ProgressDraft {
  content: string
  author: string
  saving: boolean
  editingId: string | null
  editingText: string
  editingSaving: boolean
}

export const useTaskStore = defineStore('tasks', () => {
  const toast = useToastStore()
  const confirm = useConfirmStore()
  const vault = useVaultStore()
  const lastAuthor = ref('')
  const touchedAuthors = new Set<string>()
  let lastAuthorLoaded = false
  let authorVersion = 0
  let authorLoad: Promise<void> | null = null
  const view = ref<ViewKey>('all')
  const tasks = ref<Task[]>([])
  const counts = ref<Counts>({ ...EMPTY_COUNTS })
  const search = ref('')
  const priorityFilter = ref<number | null>(null)
  const sort = ref<SortKey>('priority')
  const dateFrom = ref<string | null>(null)
  const dateTo = ref<string | null>(null)
  const dateError = ref<string | null>(null)
  const dateFilter = computed(() =>
    dateFrom.value === dateTo.value ? dateFrom.value : null
  )
  const page = ref(1)
  const pageSize = ref(50)
  const total = ref(0)
  const pageCount = computed(() =>
    Math.max(1, Math.ceil(total.value / pageSize.value))
  )
  const loading = ref(false)
  const ready = ref(false)
  const error = ref<string | null>(null)
  const selectedId = ref<string | null>(null)
  const detail = ref<TaskDetail | null>(null)
  const detailLoading = ref(false)
  const busy = ref<Record<string, boolean>>({})
  const selectedIds = ref<string[]>([])
  const batchBusy = ref(false)
  const batchError = ref<string | null>(null)
  const updatesByTask = ref<Record<string, TaskUpdate[]>>({})
  const updatesLoading = ref<Record<string, boolean>>({})
  const progressDrafts = ref<Record<string, ProgressDraft>>({})
  let listVersion = 0
  let detailVersion = 0
  let dataEpoch = 0
  let updatesVersions: Record<string, number> = {}
  let busyCounts: Record<string, number> = {}
  const updatesRequests = new Map<string, Promise<TaskUpdate[] | null>>()

  const isSearching = computed(() => search.value.trim().length > 0)
  const effectiveView = computed<ViewKey>(() =>
    view.value === 'today' ? 'today' : isSearching.value ? 'search' : view.value
  )
  const selectedTask = computed<Task | null>(
    () =>
      detail.value?.task ??
      tasks.value.find((item) => item.id === selectedId.value) ??
      null
  )
  const allPageSelected = computed(
    () =>
      tasks.value.length > 0 &&
      tasks.value.every((task) => selectedIds.value.includes(task.id))
  )
  const reorderReason = computed(() => {
    if (batchBusy.value) return '批量操作提交期间暂停排序'
    if (loading.value) return '正在读取事项，完成后可继续排序'
    if (sort.value !== 'manual') return '切换为「手动排序」后可拖动事项'
    if (
      isSearching.value ||
      priorityFilter.value !== null ||
      dateFrom.value ||
      dateTo.value ||
      view.value === 'today'
    )
      return '清除搜索、优先级和记录日期条件后可拖动排序；今日视图请切换到全部事项'
    return ''
  })
  const canReorder = computed(() => reorderReason.value === '')

  function buildFilter(): TaskFilter {
    const useDates = view.value !== 'today'
    return {
      view: effectiveView.value,
      priority: priorityFilter.value,
      search: isSearching.value ? search.value.trim() : null,
      sort: sort.value,
      limit: pageSize.value,
      offset: (page.value - 1) * pageSize.value,
      dateFrom:
        useDates && dateFrom.value
          ? (localDayBounds(dateFrom.value)?.from ?? null)
          : null,
      dateTo:
        useDates && dateTo.value
          ? (localDayBounds(dateTo.value)?.to ?? null)
          : null,
    }
  }
  async function refresh(options: { quiet?: boolean } = {}) {
    const request = ++listVersion
    if (dateError.value && view.value !== 'today') {
      loading.value = false
      return
    }
    const filter = buildFilter()
    loading.value = true
    try {
      const [result, nextCounts] = await Promise.all([
        api.pageTasks(filter),
        api.counts(),
      ])
      if (request !== listVersion) return
      const lastPage = Math.max(1, Math.ceil(result.total / pageSize.value))
      if (page.value > lastPage) {
        page.value = lastPage
        selectedIds.value = []
        await refresh(options)
        return
      }
      tasks.value = result.items
      total.value = result.total
      counts.value = nextCounts
      error.value = null
      if (!batchBusy.value)
        selectedIds.value = selectedIds.value.filter((id) =>
          result.items.some((item) => item.id === id)
        )
    } catch (caught) {
      if (request !== listVersion) return
      error.value = toApiFailure(caught).message
      if (!options.quiet) toast.error(error.value)
    } finally {
      if (request === listVersion) {
        loading.value = false
        ready.value = true
      }
    }
  }
  async function bootstrap() {
    await Promise.all([loadLastAuthor(), refresh({ quiet: true })])
  }
  async function loadLastAuthor() {
    if (!vault.unlocked || lastAuthorLoaded) return
    if (authorLoad) return authorLoad
    const epoch = dataEpoch
    const version = authorVersion
    const previous = lastAuthor.value
    const current = () => vault.unlocked && epoch === dataEpoch && version === authorVersion
    const pending = (async () => {
      try {
        const saved = await api.settingsGet('last_author')
        if (!current()) return
        const legacy = localStorage.getItem('shiftnote:last-author')
        const value = saved ?? legacy ?? ''
        lastAuthor.value = value
        for (const [taskId, draft] of Object.entries(progressDrafts.value)) {
          if (!touchedAuthors.has(taskId) && draft.author === previous) draft.author = value
        }
        if (legacy !== null) {
          // A failed migration keeps the old value available for the next retry.
          await api.settingsSet('last_author', value)
          if (!current()) return
          localStorage.removeItem('shiftnote:last-author')
        }
        lastAuthorLoaded = true
      } catch (caught) {
        if (current()) toast.error(toApiFailure(caught).message)
      }
    })()
    authorLoad = pending
    try {
      await pending
    } finally {
      if (authorLoad === pending) authorLoad = null
    }
  }
  async function saveLastAuthor(value: string) {
    if (!vault.unlocked) return
    const epoch = dataEpoch
    const version = ++authorVersion
    lastAuthor.value = value
    lastAuthorLoaded = true
    try {
      await api.settingsSet('last_author', value)
      if (vault.unlocked && epoch === dataEpoch && version === authorVersion)
        localStorage.removeItem('shiftnote:last-author')
    } catch (caught) {
      if (epoch === dataEpoch) toast.error(toApiFailure(caught).message)
    }
  }
  function clearSelection() {
    if (batchBusy.value) return
    selectedIds.value = []
    batchError.value = null
  }
  function resetScope() {
    page.value = 1
    clearSelection()
  }
  async function setView(next: ViewKey) {
    if (batchBusy.value) return
    view.value = next
    resetScope()
    await selectTask(null)
    await refresh()
  }
  async function syncView(next: ViewKey) {
    if (view.value === next || batchBusy.value) return
    view.value = next
    resetScope()
    await selectTask(null)
    if (ready.value) await refresh()
  }
  async function setSearch(value: string) {
    if (batchBusy.value) return
    search.value = value
    resetScope()
    await refresh({ quiet: true })
  }
  async function setPriorityFilter(value: number | null) {
    if (batchBusy.value) return
    priorityFilter.value = value
    resetScope()
    await refresh({ quiet: true })
  }
  async function setSort(value: SortKey) {
    if (batchBusy.value) return
    sort.value = value
    resetScope()
    await refresh({ quiet: true })
  }
  function validDate(value: string | null) {
    if (!value) return true
    const bounds = localDayBounds(value)
    if (!bounds) return false
    const d = new Date(bounds.from)
    return (
      `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}` ===
      value
    )
  }
  async function setDateRange(from: string | null, to: string | null) {
    if (batchBusy.value) return
    dateFrom.value = from?.trim() || null
    dateTo.value = to?.trim() || null
    resetScope()
    dateError.value =
      !validDate(dateFrom.value) || !validDate(dateTo.value)
        ? '请输入有效的记录日期'
        : dateFrom.value && dateTo.value && dateFrom.value > dateTo.value
          ? '开始日期不能晚于结束日期，筛选尚未应用'
          : null
    await refresh({ quiet: true })
  }
  async function setDateFilter(value: string | null) {
    await setDateRange(value, value)
  }
  async function setPage(value: number) {
    if (batchBusy.value) return
    page.value = Math.max(1, Math.floor(value))
    clearSelection()
    await refresh({ quiet: true })
  }
  async function setPageSize(value: number) {
    if (batchBusy.value) return
    pageSize.value = Math.min(500, Math.max(1, Math.floor(value)))
    resetScope()
    await refresh({ quiet: true })
  }

  async function loadDetail(id: string | null = selectedId.value) {
    const request = ++detailVersion
    if (!id || id !== selectedId.value) {
      detail.value = null
      detailLoading.value = false
      return
    }
    const historyVersion = updatesVersions[id] ?? 0
    detailLoading.value = true
    try {
      const result = await api.getTask(id)
      if (request !== detailVersion || selectedId.value !== id) return
      detail.value = result
      if ((updatesVersions[id] ?? 0) === historyVersion)
        updatesByTask.value[id] = result.updates
    } catch (caught) {
      if (request !== detailVersion || selectedId.value !== id) return
      toast.error(toApiFailure(caught).message)
      detail.value = null
      selectedId.value = null
    } finally {
      if (request === detailVersion) detailLoading.value = false
    }
  }
  async function selectTask(id: string | null) {
    ++detailVersion
    selectedId.value = id
    detail.value = null
    detailLoading.value = false
    if (id) await loadDetail(id)
  }
  /** Invalidate old reads immediately after database replacement, before bootstrap. */
  function resetAfterRestore() {
    ++dataEpoch
    ++listVersion
    ++detailVersion
    tasks.value = []
    total.value = 0
    counts.value = { ...EMPTY_COUNTS }
    selectedId.value = null
    detail.value = null
    detailLoading.value = false
    loading.value = false
    ready.value = false
    error.value = null
    selectedIds.value = []
    batchBusy.value = false
    batchError.value = null
    updatesByTask.value = {}
    updatesLoading.value = {}
    progressDrafts.value = {}
    lastAuthor.value = ''
    lastAuthorLoaded = false
    ++authorVersion
    authorLoad = null
    touchedAuthors.clear()
    updatesVersions = {}
    busyCounts = {}
    busy.value = {}
    updatesRequests.clear()
    page.value = 1
  }
  function setBusy(id: string, value: boolean) {
    busyCounts[id] = Math.max(0, (busyCounts[id] ?? 0) + (value ? 1 : -1))
    if (busyCounts[id]) busy.value[id] = true
    else delete busy.value[id]
  }
  async function mutate<T>(
    id: string,
    action: () => Promise<T>,
    options: { closeDetail?: boolean } = {}
  ): Promise<T | null> {
    if (batchBusy.value) return null
    const epoch = dataEpoch
    setBusy(id, true)
    try {
      const result = await action()
      if (epoch !== dataEpoch) return null
      if (options.closeDetail && selectedId.value === id) await selectTask(null)
      await refresh({ quiet: true })
      if (epoch !== dataEpoch) return null
      if (selectedId.value === id) await loadDetail(id)
      return epoch === dataEpoch ? result : null
    } catch (caught) {
      if (epoch === dataEpoch) toast.error(toApiFailure(caught).message)
      return null
    } finally {
      if (epoch === dataEpoch) setBusy(id, false)
    }
  }
  async function createTask(
    input: CreateTaskInput,
    options: { select?: boolean } = {}
  ): Promise<Task | null> {
    if (batchBusy.value) return null
    const epoch = dataEpoch
    try {
      const task = await api.createTask(input)
      if (epoch !== dataEpoch) return null
      await refresh({ quiet: true })
      if (epoch !== dataEpoch) return null
      if (options.select !== false) await selectTask(task.id)
      if (epoch !== dataEpoch) return null
      toast.success('事项已添加')
      return task
    } catch (caught) {
      if (epoch === dataEpoch) toast.error(toApiFailure(caught).message)
      return null
    }
  }
  async function updateTask(id: string, input: CreateTaskInput) {
    return mutate(id, () =>
      api.updateTask(id, {
        title: input.title,
        description: input.description ?? '',
        priority: input.priority,
        department: input.department ?? '',
        recordedAt: input.recordedAt ?? 0,
      })
    )
  }
  async function toggleCompleted(task: Task) {
    const completed = task.status !== 'done'
    const result = await mutate(task.id, () =>
      api.setCompleted(task.id, completed)
    )
    if (result)
      toast.success(completed ? '已标记为完成，提醒已停止' : '已恢复为未完成')
    return result
  }
  async function changePriority(id: string, priority: number) {
    return mutate(id, () => api.setPriority(id, priority))
  }
  async function hideTask(task: Task) {
    const result = await mutate(task.id, () => api.setHidden(task.id, true))
    if (result) {
      if (selectedId.value === task.id) await selectTask(null)
      toast.success('已隐藏，可在「已隐藏事项」中找回')
    }
    return result
  }
  async function unhideTask(task: Task) {
    const result = await mutate(task.id, () => api.setHidden(task.id, false))
    if (result) toast.success('已恢复到列表')
    return result
  }
  async function archiveTask(task: Task, archived = true) {
    if (archived && task.status !== 'done') {
      toast.error('只有已完成事项可以归档')
      return null
    }
    const result = await mutate(task.id, () =>
      api.setArchived(task.id, archived)
    )
    if (result) {
      if (selectedId.value === task.id) await selectTask(null)
      toast.success(
        archived ? '已归档，可在「已归档事项」中找回' : '已取消归档'
      )
    }
    return result
  }
  async function trashTask(task: Task) {
    if (batchBusy.value) return null
    const ok = await confirm.ask({
      title: '移入回收站？',
      message: `「${task.title}」及其全部进展记录会保留在回收站，可随时恢复。`,
      confirmText: '移入回收站',
    })
    if (!ok) return null
    const result = await mutate(task.id, async () => {
      await api.trashTask(task.id)
      return true
    })
    if (result) {
      if (selectedId.value === task.id) await selectTask(null)
      toast.success('已移入回收站')
    }
    return result
  }
  async function restoreTask(task: Task) {
    const result = await mutate(task.id, () => api.restoreTask(task.id))
    if (result) {
      if (selectedId.value === task.id) await selectTask(null)
      toast.success('已恢复到全部事项')
    }
    return result
  }
  async function deletePermanently(task: Task) {
    if (batchBusy.value) return null
    const ok = await confirm.ask({
      title: '永久删除？',
      message: `「${task.title}」及其 ${task.updateCount} 条进展记录将被彻底删除，无法恢复。`,
      confirmText: '永久删除',
      danger: true,
    })
    if (!ok) return null
    const result = await mutate(
      task.id,
      async () => {
        await api.deleteTaskPermanent(task.id)
        return true
      },
      { closeDetail: true }
    )
    if (result) {
      if (selectedId.value === task.id) await selectTask(null)
      toast.success('已永久删除')
    }
    return result
  }
  async function duplicateTask(task: Task) {
    const copy = await mutate(task.id, () => api.duplicateTask(task.id))
    if (copy) {
      await selectTask(copy.id)
      toast.success('已创建副本')
    }
    return copy
  }

  function invalidateUpdates(taskId: string) {
    updatesVersions[taskId] = (updatesVersions[taskId] ?? 0) + 1
    updatesRequests.delete(taskId)
  }
  async function loadUpdates(
    taskId: string,
    options: { force?: boolean } = {}
  ): Promise<TaskUpdate[] | null> {
    if (!options.force && updatesRequests.has(taskId))
      return updatesRequests.get(taskId)!
    if (!options.force && updatesByTask.value[taskId])
      return updatesByTask.value[taskId]
    const epoch = dataEpoch
    const request = (updatesVersions[taskId] ?? 0) + 1
    updatesVersions[taskId] = request
    updatesLoading.value[taskId] = true
    const current = () =>
      epoch === dataEpoch && updatesVersions[taskId] === request
    const pending = (async () => {
      try {
        const items = await api.listUpdates(taskId)
        if (!current()) return null
        updatesByTask.value[taskId] = items
        return items
      } catch (caught) {
        if (current()) toast.error(toApiFailure(caught).message)
        return null
      } finally {
        if (current()) {
          updatesLoading.value[taskId] = false
          updatesRequests.delete(taskId)
        }
      }
    })()
    updatesRequests.set(taskId, pending)
    return pending
  }
  async function progressMutation<T>(
    taskId: string,
    action: () => Promise<T>
  ): Promise<T | null> {
    if (batchBusy.value) return null
    const epoch = dataEpoch
    setBusy(taskId, true)
    try {
      const result = await action()
      if (epoch !== dataEpoch) return null
      invalidateUpdates(taskId)
      await Promise.all([
        refresh({ quiet: true }),
        loadUpdates(taskId, { force: true }),
        selectedId.value === taskId ? loadDetail(taskId) : Promise.resolve(),
      ])
      return epoch === dataEpoch ? result : null
    } catch (caught) {
      if (epoch === dataEpoch) toast.error(toApiFailure(caught).message)
      return null
    } finally {
      if (epoch === dataEpoch) setBusy(taskId, false)
    }
  }
  async function addProgress(
    taskId: string,
    content: string,
    author?: string | null
  ) {
    const created = await progressMutation(taskId, () =>
      api.addUpdate(taskId, content, author)
    )
    if (created) await saveLastAuthor(author ?? '')
    return created
  }
  async function editProgress(
    taskId: string,
    updateId: string,
    content: string
  ) {
    const item = await progressMutation(taskId, () =>
      api.editUpdate(updateId, content)
    )
    if (item) toast.success('进展已更新')
    return item
  }
  async function removeProgress(taskId: string, update: TaskUpdate) {
    if (batchBusy.value) return false
    const ok = await confirm.ask({
      title: '删除这条进展？',
      message: '删除后无法恢复，其余进展记录不受影响。',
      confirmText: '删除',
      danger: true,
    })
    if (!ok) return false
    const result = await progressMutation(taskId, async () => {
      await api.deleteUpdate(update.id)
      return true
    })
    if (result) toast.success('进展已删除')
    return Boolean(result)
  }
  function getProgressDraft(taskId: string): ProgressDraft {
    if (!progressDrafts.value[taskId])
      progressDrafts.value[taskId] = {
        content: '',
        author: lastAuthor.value,
        saving: false,
        editingId: null,
        editingText: '',
        editingSaving: false,
      }
    return progressDrafts.value[taskId]
  }
  function setProgressAuthor(taskId: string, value: string) {
    touchedAuthors.add(taskId)
    getProgressDraft(taskId).author = value
  }
  function toggleSelection(id: string) {
    if (
      batchBusy.value ||
      loading.value ||
      !tasks.value.some((task) => task.id === id)
    )
      return
    selectedIds.value = selectedIds.value.includes(id)
      ? selectedIds.value.filter((item) => item !== id)
      : [...selectedIds.value, id]
    batchError.value = null
  }
  function selectCurrentPage() {
    if (batchBusy.value || loading.value) return
    selectedIds.value = allPageSelected.value
      ? []
      : tasks.value.map((task) => task.id)
    batchError.value = null
  }
  async function applyBatch(action: TaskBatchAction, priority?: number) {
    if (batchBusy.value || loading.value || selectedIds.value.length === 0)
      return null
    const ids = [...selectedIds.value]
    const epoch = dataEpoch
    batchBusy.value = true
    batchError.value = null
    try {
      if (action === 'deletePermanent' || action === 'trash') {
        const ok = await confirm.ask({
          title:
            action === 'deletePermanent'
              ? `永久删除 ${ids.length} 条事项？`
              : `将 ${ids.length} 条事项移入回收站？`,
          message:
            action === 'deletePermanent'
              ? `已选 ${ids.length} 条事项及其进展将被彻底删除，无法恢复。已定稿交接的快照仍会保留。`
              : `已选 ${ids.length} 条事项及进展将保留在回收站，可随时恢复。`,
          confirmText:
            action === 'deletePermanent'
              ? `永久删除 ${ids.length} 条`
              : '移入回收站',
          danger: action === 'deletePermanent',
        })
        if (!ok || epoch !== dataEpoch) return null
      }
      const result = await api.batchTasks({
        ids,
        action,
        ...(priority === undefined ? {} : { priority }),
      })
      if (epoch !== dataEpoch) return null
      if (selectedId.value && ids.includes(selectedId.value))
        await selectTask(null)
      selectedIds.value = []
      await refresh({ quiet: true })
      if (epoch !== dataEpoch) return null
      toast.success(
        `已处理 ${result.requested} 条事项：变更 ${result.changed} 条，无需变更 ${result.unchanged} 条`
      )
      return result
    } catch (caught) {
      if (epoch === dataEpoch) {
        const failure = toApiFailure(caught)
        batchError.value = `${failure.message}${failure.invalidIds.length ? `（事项：${failure.invalidIds.join('、')}）` : ''}`
        toast.error(batchError.value)
      }
      return null
    } finally {
      if (epoch === dataEpoch) batchBusy.value = false
    }
  }
  async function moveTask(
    id: string,
    anchors: { beforeId?: string | null; afterId?: string | null }
  ) {
    if (!canReorder.value) return false
    const result = await mutate(id, async () => {
      await api.moveTask({ id, ...anchors, view: view.value })
      return true
    })
    return Boolean(result)
  }
  async function moveToAdjacentPage(id: string, direction: -1 | 1) {
    if (
      !canReorder.value ||
      (direction < 0 ? page.value <= 1 : page.value >= pageCount.value)
    )
      return false
    const epoch = dataEpoch
    const request = listVersion
    const filter = buildFilter()
    try {
      const adjacent = await api.pageTasks({
        ...filter,
        offset: (page.value - 1 + direction) * pageSize.value,
      })
      if (epoch !== dataEpoch || request !== listVersion || !canReorder.value)
        return false
      const anchor =
        direction < 0
          ? adjacent.items[adjacent.items.length - 1]
          : adjacent.items[0]
      if (!anchor) return false
      return await moveTask(
        id,
        direction < 0 ? { afterId: anchor.id } : { beforeId: anchor.id }
      )
    } catch (caught) {
      if (epoch === dataEpoch && request === listVersion)
        toast.error(toApiFailure(caught).message)
      return false
    }
  }
  async function reorder(orderedIds: string[]) {
    // Compatibility only; the list persists relative full-scope moves.
    try {
      await api.reorderTasks(orderedIds)
      await refresh({ quiet: true })
      return true
    } catch (caught) {
      toast.error(toApiFailure(caught).message)
      return false
    }
  }
  return {
    view,
    effectiveView,
    tasks,
    counts,
    search,
    priorityFilter,
    sort,
    dateFilter,
    dateFrom,
    dateTo,
    dateError,
    page,
    pageSize,
    total,
    pageCount,
    loading,
    ready,
    error,
    selectedId,
    detail,
    detailLoading,
    busy,
    selectedIds,
    batchBusy,
    batchError,
    allPageSelected,
    updatesByTask,
    updatesLoading,
    progressDrafts,
    lastAuthor,
    isSearching,
    selectedTask,
    canReorder,
    reorderReason,
    bootstrap,
    loadLastAuthor,
    refresh,
    resetAfterRestore,
    setView,
    syncView,
    setSearch,
    setPriorityFilter,
    setSort,
    setDateFilter,
    setDateRange,
    setPage,
    setPageSize,
    selectTask,
    loadDetail,
    createTask,
    updateTask,
    toggleCompleted,
    changePriority,
    hideTask,
    unhideTask,
    archiveTask,
    trashTask,
    restoreTask,
    deletePermanently,
    duplicateTask,
    addProgress,
    editProgress,
    removeProgress,
    loadUpdates,
    getProgressDraft,
    setProgressAuthor,
    clearSelection,
    toggleSelection,
    selectCurrentPage,
    applyBatch,
    moveTask,
    moveToAdjacentPage,
    reorder,
  }
})
