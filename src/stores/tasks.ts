import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import { api, toApiFailure } from '@/services/api'
import { localDayBounds } from '@/services/format'
import { useConfirmStore } from '@/stores/confirm'
import { useToastStore } from '@/stores/toast'
import type {
  Counts,
  CreateTaskInput,
  SortKey,
  Task,
  TaskDetail,
  TaskFilter,
  TaskUpdate,
  ViewKey,
} from '@/types'

const EMPTY_COUNTS: Counts = { all: 0, today: 0, open: 0, focus: 0, done: 0, hidden: 0, trash: 0 }

export const useTaskStore = defineStore('tasks', () => {
  const toast = useToastStore()
  const confirm = useConfirmStore()

  const view = ref<ViewKey>('all')
  const tasks = ref<Task[]>([])
  const counts = ref<Counts>({ ...EMPTY_COUNTS })
  const search = ref('')
  const priorityFilter = ref<number | null>(null)
  const sort = ref<SortKey>('priority')
  /** 记录日期筛选（YYYY-MM-DD），null 表示不限定日期 */
  const dateFilter = ref<string | null>(null)

  const loading = ref(false)
  const ready = ref(false)
  const error = ref<string | null>(null)

  const selectedId = ref<string | null>(null)
  const detail = ref<TaskDetail | null>(null)
  const detailLoading = ref(false)
  const busy = ref<Record<string, boolean>>({})

  const isSearching = computed(() => search.value.trim().length > 0)
  const effectiveView = computed<ViewKey>(() => (isSearching.value ? 'search' : view.value))

  const selectedTask = computed<Task | null>(
    () => detail.value?.task ?? tasks.value.find((item) => item.id === selectedId.value) ?? null
  )

  function buildFilter(): TaskFilter {
    const bounds = dateFilter.value ? localDayBounds(dateFilter.value) : null
    return {
      view: effectiveView.value,
      priority: priorityFilter.value,
      search: isSearching.value ? search.value.trim() : null,
      sort: sort.value,
      limit: 500,
      dateFrom: bounds?.from ?? null,
      dateTo: bounds?.to ?? null,
    }
  }

  async function refresh(options: { quiet?: boolean } = {}) {
    if (!options.quiet) loading.value = true
    try {
      const [list, nextCounts] = await Promise.all([api.listTasks(buildFilter()), api.counts()])
      tasks.value = list
      counts.value = nextCounts
      error.value = null
    } catch (caught) {
      const failure = toApiFailure(caught)
      error.value = failure.message
      if (!options.quiet) toast.error(failure.message)
    } finally {
      loading.value = false
      ready.value = true
    }
  }

  async function bootstrap() {
    await refresh({ quiet: true })
  }

  async function setView(next: ViewKey) {
    view.value = next
    selectedId.value = null
    detail.value = null
    await refresh()
  }

  /**
   * 由路由驱动的视图同步：首次挂载时（ready=false）只设置视图，
   * 由 App 的 bootstrap 统一加载，避免重复查询。
   */
  async function syncView(next: ViewKey) {
    if (view.value === next) return
    view.value = next
    selectedId.value = null
    detail.value = null
    if (ready.value) await refresh()
  }

  async function setSearch(value: string) {
    search.value = value
    await refresh({ quiet: true })
  }

  async function setPriorityFilter(value: number | null) {
    priorityFilter.value = value
    await refresh({ quiet: true })
  }

  async function setSort(value: SortKey) {
    sort.value = value
    await refresh({ quiet: true })
  }

  /** 按记录日期筛选；传 null 清除 */
  async function setDateFilter(value: string | null) {
    const next = value && value.trim() ? value.trim() : null
    if (dateFilter.value === next) return
    dateFilter.value = next
    await refresh({ quiet: true })
  }

  async function loadDetail(id: string | null = selectedId.value) {
    if (!id) {
      detail.value = null
      return
    }
    detailLoading.value = true
    try {
      detail.value = await api.getTask(id)
    } catch (caught) {
      const failure = toApiFailure(caught)
      toast.error(failure.message)
      detail.value = null
      selectedId.value = null
    } finally {
      detailLoading.value = false
    }
  }

  async function selectTask(id: string | null) {
    selectedId.value = id
    if (!id) {
      detail.value = null
      return
    }
    await loadDetail(id)
  }

  function setBusy(id: string, value: boolean) {
    const next = { ...busy.value }
    if (value) next[id] = true
    else delete next[id]
    busy.value = next
  }

  async function mutate<T>(id: string, action: () => Promise<T>): Promise<T | null> {
    setBusy(id, true)
    try {
      const result = await action()
      await refresh({ quiet: true })
      if (selectedId.value === id) {
        await loadDetail(id)
      }
      return result
    } catch (caught) {
      toast.error(toApiFailure(caught).message)
      return null
    } finally {
      setBusy(id, false)
    }
  }

  async function createTask(
    input: CreateTaskInput,
    options: { select?: boolean } = {}
  ): Promise<Task | null> {
    try {
      const task = await api.createTask(input)
      await refresh({ quiet: true })
      if (options.select !== false) {
        selectedId.value = task.id
        await loadDetail(task.id)
      }
      toast.success('事项已添加')
      return task
    } catch (caught) {
      toast.error(toApiFailure(caught).message)
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
    const result = await mutate(task.id, () => api.setCompleted(task.id, completed))
    if (result) {
      toast.success(completed ? '已标记为完成，提醒已停止' : '已恢复为未完成')
    }
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

  async function trashTask(task: Task) {
    const ok = await confirm.ask({
      title: '移入回收站？',
      message: `「${task.title}」及其全部进展记录会保留在回收站，可随时恢复。`,
      confirmText: '移入回收站',
    })
    if (!ok) return null

    setBusy(task.id, true)
    try {
      await api.trashTask(task.id)
      if (selectedId.value === task.id) await selectTask(null)
      await refresh({ quiet: true })
      toast.success('已移入回收站')
      return true
    } catch (caught) {
      toast.error(toApiFailure(caught).message)
      return null
    } finally {
      setBusy(task.id, false)
    }
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
    const ok = await confirm.ask({
      title: '永久删除？',
      message: `「${task.title}」及其 ${task.updateCount} 条进展记录将被彻底删除，无法恢复。`,
      confirmText: '永久删除',
      danger: true,
    })
    if (!ok) return null

    setBusy(task.id, true)
    try {
      await api.deleteTaskPermanent(task.id)
      if (selectedId.value === task.id) await selectTask(null)
      await refresh({ quiet: true })
      toast.success('已永久删除')
      return true
    } catch (caught) {
      toast.error(toApiFailure(caught).message)
      return null
    } finally {
      setBusy(task.id, false)
    }
  }

  async function duplicateTask(task: Task) {
    try {
      const copy = await api.duplicateTask(task.id)
      await refresh({ quiet: true })
      selectedId.value = copy.id
      await loadDetail(copy.id)
      toast.success('已创建副本')
      return copy
    } catch (caught) {
      toast.error(toApiFailure(caught).message)
      return null
    }
  }

  async function addProgress(taskId: string, content: string, author?: string | null) {
    setBusy(taskId, true)
    try {
      const item = await api.addUpdate(taskId, content, author)
      await refresh({ quiet: true })
      if (selectedId.value === taskId) await loadDetail(taskId)
      return item
    } catch (caught) {
      toast.error(toApiFailure(caught).message)
      return null
    } finally {
      setBusy(taskId, false)
    }
  }

  async function editProgress(taskId: string, updateId: string, content: string) {
    setBusy(taskId, true)
    try {
      const item = await api.editUpdate(updateId, content)
      if (selectedId.value === taskId) await loadDetail(taskId)
      await refresh({ quiet: true })
      toast.success('进展已更新')
      return item
    } catch (caught) {
      toast.error(toApiFailure(caught).message)
      return null
    } finally {
      setBusy(taskId, false)
    }
  }

  async function removeProgress(taskId: string, update: TaskUpdate) {
    const ok = await confirm.ask({
      title: '删除这条进展？',
      message: '删除后无法恢复，其余进展记录不受影响。',
      confirmText: '删除',
      danger: true,
    })
    if (!ok) return false

    setBusy(taskId, true)
    try {
      await api.deleteUpdate(update.id)
      if (selectedId.value === taskId) await loadDetail(taskId)
      await refresh({ quiet: true })
      toast.success('进展已删除')
      return true
    } catch (caught) {
      toast.error(toApiFailure(caught).message)
      return false
    } finally {
      setBusy(taskId, false)
    }
  }

  /**
   * 读取某个事项的全部进展（用于列表行内展开历史）。
   * 列表查询只带最新一条，展开时按需拉取，避免首屏 N 次查询。
   */
  async function loadUpdates(taskId: string): Promise<TaskUpdate[] | null> {
    try {
      return await api.listUpdates(taskId)
    } catch (caught) {
      toast.error(toApiFailure(caught).message)
      return null
    }
  }

  async function reorder(orderedIds: string[]) {
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
    loading,
    ready,
    error,
    selectedId,
    detail,
    detailLoading,
    busy,
    isSearching,
    selectedTask,
    bootstrap,
    refresh,
    setView,
    syncView,
    setSearch,
    setPriorityFilter,
    setSort,
    setDateFilter,
    selectTask,
    loadDetail,
    createTask,
    updateTask,
    toggleCompleted,
    changePriority,
    hideTask,
    unhideTask,
    trashTask,
    restoreTask,
    deletePermanently,
    duplicateTask,
    addProgress,
    editProgress,
    removeProgress,
    loadUpdates,
    reorder,
  }
})
