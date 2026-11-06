import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { api } from '@/services/api'
import { useTaskStore } from '@/stores/tasks'
import { useConfirmStore } from '@/stores/confirm'
import { useToastStore } from '@/stores/toast'
import type { Counts, Task, TaskDetail, TaskUpdate } from '@/types'

vi.mock('@/services/api', async (original) => {
  const actual = await original<typeof import('@/services/api')>()
  return {
    ...actual,
    api: Object.fromEntries(
      [
        'pageTasks',
        'listTasks',
        'counts',
        'getTask',
        'listUpdates',
        'editUpdate',
        'addUpdate',
        'deleteUpdate',
        'batchTasks',
        'moveTask',
        'setArchived',
        'deleteTaskPermanent',
      ].map((name) => [name, vi.fn()])
    ),
  }
})

const counts = {
  all: 0,
  today: 0,
  open: 0,
  focus: 0,
  done: 0,
  hidden: 0,
  trash: 0,
  archived: 0,
} as Counts
function task(id: string): Task {
  return {
    id,
    title: id,
    description: null,
    priority: 2,
    status: 'open',
    completedAt: null,
    hidden: false,
    archived: false,
    deletedAt: null,
    categoryId: null,
    department: null,
    recordedAt: 1,
    sortOrder: 1,
    createdAt: 1,
    updatedAt: 1,
    latestUpdateContent: null,
    latestUpdateAt: null,
    updateCount: 0,
    reminderEnabled: false,
    nextRemindAt: null,
  }
}
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((yes, no) => {
    resolve = yes
    reject = no
  })
  return { promise, resolve, reject }
}
const page = (ids: string[], total = ids.length, offset = 0) => ({
  items: ids.map(task),
  total,
  limit: 50,
  offset,
})
const detail = (id: string): TaskDetail => ({ task: task(id), updates: [] })

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.mocked(api.pageTasks).mockResolvedValue(page([]))
  vi.mocked(api.listTasks).mockResolvedValue([])
  vi.mocked(api.counts).mockResolvedValue(counts)
  vi.mocked(api.getTask).mockImplementation(async (id) => detail(id))
  vi.mocked(api.listUpdates).mockResolvedValue([])
})

describe('T01 request ownership', () => {
  it('T12 twenty rapid selections keep only the twentieth detail after mixed out-of-order successes and failures', async () => {
    const store = useTaskStore()
    const responses = Array.from({ length: 20 }, () => deferred<TaskDetail>())
    vi.mocked(api.getTask).mockImplementation(
      (id) => responses[Number(id) - 1]!.promise
    )
    const selections = Array.from({ length: 20 }, (_, index) =>
      store.selectTask(String(index + 1))
    )
    expect(api.getTask).toHaveBeenCalledTimes(20)
    expect(store.selectedId).toBe('20')
    expect(store.detail).toBeNull()

    async function settle(index: number) {
      if (index % 3 === 0)
        responses[index]!.reject(new Error(`过期错误 ${index + 1}`))
      else responses[index]!.resolve(detail(String(index + 1)))
      await selections[index]
    }
    for (const index of [6, 2, 17, 0, 8, 12, 4, 15, 10]) {
      await settle(index)
      expect(store.selectedId).toBe('20')
      expect(store.detail).toBeNull()
      expect(store.detailLoading).toBe(true)
      expect(useToastStore().items).toEqual([])
    }

    responses[19]!.resolve(detail('20'))
    await selections[19]
    for (const index of [18, 1, 3, 5, 7, 9, 11, 13, 14, 16]) {
      await settle(index)
      expect(store.selectedId).toBe('20')
      expect(store.detail?.task.id).toBe('20')
      expect(store.detailLoading).toBe(false)
      expect(useToastStore().items).toEqual([])
    }
    await Promise.all(selections)
    expect(store.updatesByTask).toEqual({ '20': [] })
  })

  it('T12 an old list failure cannot stop a newer pending query or overwrite its eventual error', async () => {
    const store = useTaskStore()
    const old = deferred<ReturnType<typeof page>>()
    const current = deferred<ReturnType<typeof page>>()
    vi.mocked(api.pageTasks)
      .mockReturnValueOnce(old.promise)
      .mockReturnValueOnce(current.promise)
    const first = store.refresh()
    const second = store.setSearch('current')
    old.reject(new Error('旧请求错误'))
    await first
    expect(store.loading).toBe(true)
    expect(store.error).toBeNull()
    expect(useToastStore().items).toEqual([])
    current.reject(new Error('当前查询错误'))
    await second
    expect(store.error).toBe('当前查询错误')
    expect(store.loading).toBe(false)

    const nextOld = deferred<ReturnType<typeof page>>()
    const nextCurrent = deferred<ReturnType<typeof page>>()
    vi.mocked(api.pageTasks)
      .mockReturnValueOnce(nextOld.promise)
      .mockReturnValueOnce(nextCurrent.promise)
    const third = store.refresh()
    const fourth = store.setSearch('latest')
    nextCurrent.reject(new Error('最新查询错误'))
    await fourth
    nextOld.reject(new Error('更晚返回的过期错误'))
    await third
    expect(store.error).toBe('最新查询错误')
    expect(store.loading).toBe(false)
    expect(useToastStore().items).toEqual([])
  })

  it('a stale list cannot replace a newer query or end its loading state', async () => {
    const store = useTaskStore()
    const old = deferred<ReturnType<typeof page>>()
    const current = deferred<ReturnType<typeof page>>()
    vi.mocked(api.pageTasks)
      .mockReturnValueOnce(old.promise)
      .mockReturnValueOnce(current.promise)
    const first = store.refresh()
    const second = store.setSearch('new')
    old.resolve(page(['old']))
    await first
    expect(store.tasks).toEqual([])
    expect(store.loading).toBe(true)
    current.resolve(page(['new']))
    await second
    expect(store.tasks.map((item) => item.id)).toEqual(['new'])
    expect(store.loading).toBe(false)
  })

  it('a stale failed detail cannot clear the current detail or show its error', async () => {
    const store = useTaskStore()
    const old = deferred<TaskDetail>()
    vi.mocked(api.getTask).mockReturnValueOnce(old.promise)
    const first = store.selectTask('a')
    await store.selectTask('b')
    old.reject(new Error('old error'))
    await first
    expect(store.selectedId).toBe('b')
    expect(store.detail?.task.id).toBe('b')
    expect(useToastStore().items).toEqual([])
  })

  it('clears the previous writable detail immediately and closing invalidates pending reads', async () => {
    const store = useTaskStore()
    await store.selectTask('a')
    const pending = deferred<TaskDetail>()
    vi.mocked(api.getTask).mockReturnValueOnce(pending.promise)
    const next = store.selectTask('b')
    expect(store.detail).toBeNull()
    await store.selectTask(null)
    pending.resolve(detail('b'))
    await next
    expect(store.detail).toBeNull()
    expect(store.detailLoading).toBe(false)
  })

  it('database restore invalidates pending list, detail and history responses', async () => {
    const store = useTaskStore()
    const pending = deferred<ReturnType<typeof page>>()
    const pendingDetail = deferred<TaskDetail>()
    const pendingHistory = deferred<TaskUpdate[]>()
    vi.mocked(api.pageTasks).mockReturnValueOnce(pending.promise)
    vi.mocked(api.getTask).mockReturnValueOnce(pendingDetail.promise)
    vi.mocked(api.listUpdates).mockReturnValueOnce(pendingHistory.promise)
    const reads = [
      store.refresh(),
      store.selectTask('a'),
      store.loadUpdates('a'),
    ]
    store.resetAfterRestore()
    pending.resolve(page(['old']))
    pendingDetail.resolve(detail('a'))
    pendingHistory.resolve([{ id: 'old' } as TaskUpdate])
    await Promise.all(reads)
    expect(store.tasks).toEqual([])
    expect(store.detail).toBeNull()
    expect(store.updatesByTask).toEqual({})
  })
})

describe('T02/T03/T06 visible date scope and full pagination', () => {
  it('today search keeps today scope and ignores a saved custom date, restored on leaving today', async () => {
    const store = useTaskStore()
    await store.setDateFilter('2026-11-05')
    await store.setView('today')
    await store.setSearch('故障')
    expect(vi.mocked(api.pageTasks).mock.lastCall?.[0]).toMatchObject({
      view: 'today',
      search: '故障',
      dateFrom: null,
      dateTo: null,
    })
    await store.setView('all')
    expect(vi.mocked(api.pageTasks).mock.lastCall?.[0]).toMatchObject({
      dateFrom: new Date(2026, 10, 5).getTime(),
      dateTo: new Date(2026, 10, 6).getTime(),
    })
  })

  it('includes the entire final local day across a year boundary and rejects reverse ranges', async () => {
    const store = useTaskStore()
    await store.setDateRange('2026-12-31', '2027-01-01')
    expect(vi.mocked(api.pageTasks).mock.lastCall?.[0]).toMatchObject({
      dateFrom: new Date(2026, 11, 31).getTime(),
      dateTo: new Date(2027, 0, 2).getTime(),
    })
    const calls = vi.mocked(api.pageTasks).mock.calls.length
    await store.setDateRange('2027-01-02', '2027-01-01')
    expect(api.pageTasks).toHaveBeenCalledTimes(calls)
    expect(store.dateError).toContain('开始日期')
  })

  it('can traverse 1001 results with no lost final page and clamps an emptied last page', async () => {
    const store = useTaskStore()
    let total = 1001
    vi.mocked(api.pageTasks).mockImplementation(async (filter) => {
      const offset = filter.offset ?? 0
      return page(
        Array.from(
          { length: Math.max(0, Math.min(50, total - offset)) },
          (_, i) => String(offset + i)
        ),
        total,
        offset
      )
    })
    const seen: string[] = []
    for (let index = 1; index <= 21; index++) {
      await store.setPage(index)
      seen.push(...store.tasks.map((item) => item.id))
    }
    expect(new Set(seen).size).toBe(1001)
    expect(store.total).toBe(1001)
    total = 1000
    await store.refresh()
    expect(store.page).toBe(20)
    expect(store.tasks).toHaveLength(50)
  })
})

describe('T05 selection and atomic batch interaction', () => {
  it('drops selections that leave the current page after a successful single-item mutation refresh', async () => {
    const store = useTaskStore()
    vi.mocked(api.pageTasks)
      .mockResolvedValueOnce(page(['a', 'b']))
      .mockResolvedValueOnce(page(['b']))
    await store.refresh()
    store.selectCurrentPage()
    await store.refresh()
    expect(store.selectedIds).toEqual(['b'])
  })

  it('selects only this page, clears on filters, locks pending scope and preserves a failed selection', async () => {
    const store = useTaskStore()
    vi.mocked(api.pageTasks).mockResolvedValue(page(['a', 'b'], 100))
    await store.refresh()
    store.selectCurrentPage()
    expect(store.selectedIds).toEqual(['a', 'b'])
    const pending = deferred<{
      requested: number
      changed: number
      unchanged: number
    }>()
    vi.mocked(api.batchTasks).mockReturnValueOnce(pending.promise)
    const batch = store.applyBatch('complete')
    await store.setSearch('else')
    store.toggleSelection('a')
    expect(store.search).toBe('')
    expect(store.selectedIds).toEqual(['a', 'b'])
    pending.reject({ message: '事务失败', invalidIds: ['b'] })
    await batch
    expect(store.selectedIds).toEqual(['a', 'b'])
    expect(store.batchError).toContain('b')
    await store.setPriorityFilter(0)
    expect(store.selectedIds).toEqual([])
  })

  it('successful batches refresh counts and clear selection; permanent deletion confirms the captured count', async () => {
    const store = useTaskStore()
    vi.mocked(api.pageTasks).mockResolvedValue(page(['a', 'b']))
    await store.refresh()
    store.selectCurrentPage()
    vi.mocked(api.batchTasks).mockResolvedValue({
      requested: 2,
      changed: 1,
      unchanged: 1,
    })
    const ask = vi.spyOn(useConfirmStore(), 'ask').mockResolvedValue(false)
    expect(await store.applyBatch('deletePermanent')).toBeNull()
    expect(ask.mock.lastCall?.[0].message).toContain('2')
    expect(api.batchTasks).not.toHaveBeenCalled()
    await store.applyBatch('complete')
    expect(store.selectedIds).toEqual([])
    expect(api.counts).toHaveBeenCalledTimes(2)
  })
})

describe('T02 shared history and T10 safe manual move', () => {
  it('does not refetch a permanently deleted selected task or report its successful deletion as a failure', async () => {
    const store = useTaskStore()
    await store.selectTask('a')
    vi.spyOn(useConfirmStore(), 'ask').mockResolvedValue(true)
    vi.mocked(api.getTask).mockRejectedValue(new Error('事项不存在'))
    await store.deletePermanently({ ...task('a'), deletedAt: 1 })
    expect(store.selectedId).toBeNull()
    expect(api.getTask).toHaveBeenCalledTimes(1)
    expect(useToastStore().items.every((item) => item.kind !== 'error')).toBe(
      true
    )
  })

  it('editing an old progress entry refreshes the already cached full history', async () => {
    const store = useTaskStore()
    const item = { id: 'old', taskId: 'a', content: 'old' } as TaskUpdate
    vi.mocked(api.listUpdates)
      .mockResolvedValueOnce([item])
      .mockResolvedValueOnce([{ ...item, content: 'new' }])
    vi.mocked(api.editUpdate).mockResolvedValue({ ...item, content: 'new' })
    await store.loadUpdates('a')
    await store.editProgress('a', item.id, 'new')
    expect(store.updatesByTask.a?.[0].content).toBe('new')
  })

  it('uses relative full-scope moves and blocks dragging under active filters', async () => {
    const store = useTaskStore()
    await store.setSort('manual')
    await store.moveTask('a', { beforeId: 'b' })
    expect(api.moveTask).toHaveBeenCalledWith({
      id: 'a',
      beforeId: 'b',
      view: 'all',
    })
    await store.setPriorityFilter(0)
    expect(store.canReorder).toBe(false)
    await store.moveTask('a', { afterId: 'b' })
    expect(api.moveTask).toHaveBeenCalledTimes(1)
  })
})
