import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { api, ApiFailure } from '@/services/api'
import { save } from '@tauri-apps/plugin-dialog'
import { useHandoverStore } from '@/stores/handovers'
import type { HandoverDetail, Task } from '@/types'

vi.mock('@/services/api', async (load) => {
  const actual = await load<typeof import('@/services/api')>()
  return {
    ...actual,
    api: Object.fromEntries(
      Object.keys(actual.api).map((key) => [key, vi.fn()])
    ),
  }
})
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: vi.fn() }))
const task = (id: string) =>
  ({
    id,
    title: id,
    status: 'open',
    priority: 1,
    recordedAt: 1,
    updateCount: 0,
  }) as Task
const record = (id: string): HandoverDetail => ({
  handover: {
    id,
    title: id,
    periodStart: 100,
    periodEnd: 200,
    status: 'draft',
    handoverDate: '2026-11-06',
    note: null,
    shiftFrom: null,
    shiftTo: null,
    finalizedAt: null,
    snapshotVersion: 1,
    createdAt: 1,
    updatedAt: 1,
  },
  items: [],
})
function deferred<T>() {
  let resolve!: (v: T) => void
  const promise = new Promise<T>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

describe('handover workflow', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.resetAllMocks()
  })
  it('a slow previous handover cannot replace the current record', async () => {
    const a = deferred<HandoverDetail>()
    vi.mocked(api.getHandover)
      .mockReturnValueOnce(a.promise)
      .mockResolvedValueOnce(record('B'))
    const store = useHandoverStore()
    const pending = store.open('A')
    await store.open('B')
    a.resolve(record('A'))
    await pending
    expect(store.form.id).toBe('B')
  })
  it('deduplicates seed IDs and selects only progress within the half-open period', async () => {
    vi.mocked(api.getTask).mockResolvedValue({ task: task('A'), updates: [] })
    const store = useHandoverStore()
    await store.startDraft(['A', 'A'])
    expect(api.getTask).toHaveBeenCalledTimes(1)
    expect(store.form.items.map((item) => item.taskId)).toEqual(['A'])
  })
  it('queries every candidate page independently of the task list', async () => {
    vi.mocked(api.handoverCandidates).mockResolvedValue({
      items: [task('1001')],
      total: 1001,
      limit: 50,
      offset: 1000,
    })
    const store = useHandoverStore()
    await store.loadCandidates(21)
    expect(api.handoverCandidates).toHaveBeenCalledWith(
      expect.objectContaining({ offset: 1000, limit: 50 })
    )
    expect(store.candidateTotal).toBe(1001)
    expect(store.candidates[0]?.id).toBe('1001')
  })
  it('initializes selected updates using [periodStart, periodEnd)', async () => {
    const store = useHandoverStore()
    store.form.periodStart = 100
    store.form.periodEnd = 200
    vi.mocked(api.getTask).mockResolvedValue({
      task: task('A'),
      updates: [99, 100, 199, 200].map((time) => ({
        id: String(time),
        taskId: 'A',
        content: '进展',
        createdAt: time,
        kind: 'progress',
        updatedAt: time,
        author: null,
        editedAt: null,
      })),
    })
    await store.addCandidate(task('A'))
    expect(store.form.items[0]?.selectedUpdateIds).toEqual(['100', '199'])
  })
  it('preserves notes and selected items after a failed save', async () => {
    const store = useHandoverStore()
    store.form.title = '夜班'
    store.form.note = '不能丢失'
    store.form.items = [
      { taskId: 'A', section: 'pending', note: '跟进', selectedUpdateIds: [] },
    ]
    vi.mocked(api.saveHandover).mockRejectedValue(new Error('磁盘失败'))
    expect(await store.saveDraft()).toBeNull()
    expect(api.saveHandover).toHaveBeenCalledOnce()
    expect(store.form.note).toBe('不能丢失')
    expect(store.form.items).toHaveLength(1)
  })
  it('does not finalize when saving the current draft fails', async () => {
    const store = useHandoverStore()
    store.form.title = '夜班'
    vi.mocked(api.saveHandover).mockRejectedValue(new Error('失败'))
    await store.finalize()
    expect(api.saveHandover).toHaveBeenCalledOnce()
    expect(api.finalizeHandover).not.toHaveBeenCalled()
  })
  it('rejects an inverted shift interval without sending a save', async () => {
    const store = useHandoverStore()
    store.form.title = '夜班'
    store.form.periodStart = 200
    store.form.periodEnd = 100
    await store.saveDraft()
    expect(api.saveHandover).not.toHaveBeenCalled()
    expect((store as unknown as { error: string | null }).error).toContain(
      '时间'
    )
  })
  it('cancelling the save dialog performs no IO and creates no error', async () => {
    vi.mocked(api.getHandover).mockResolvedValue({
      ...record('A'),
      handover: { ...record('A').handover, status: 'finalized' },
    })
    vi.mocked(save).mockResolvedValue(null)
    const store = useHandoverStore()
    await store.open('A')
    await store.exportFile('txt')
    expect(save).toHaveBeenCalledOnce()
    expect(api.exportHandover).not.toHaveBeenCalled()
    expect((store as unknown as { error: string | null }).error).toBeNull()
  })
  it('reports export IO errors while keeping the frozen record', async () => {
    vi.mocked(api.getHandover).mockResolvedValue({
      ...record('A'),
      handover: { ...record('A').handover, status: 'finalized' },
    })
    vi.mocked(save).mockResolvedValue('C:\\readonly\\夜班.txt')
    vi.mocked(api.exportHandover).mockRejectedValue(
      new ApiFailure('文件被占用', 'io')
    )
    const store = useHandoverStore()
    await store.open('A')
    await store.exportFile('txt')
    expect(api.exportHandover).toHaveBeenCalledOnce()
    expect(store.form.id).toBe('A')
    expect((store as unknown as { error: string | null }).error).toContain(
      '文件被占用'
    )
  })
})
