import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { createMemoryHistory, createRouter } from 'vue-router'
import HandoverItems from '@/components/handover/HandoverItems.vue'
import { api } from '@/services/api'
import { useHandoverStore } from '@/stores/handovers'
import type { HandoverDetail, Task, TaskSnapshot, TaskUpdate } from '@/types'

vi.mock('@/services/api', async (load) => {
  const actual = await load<typeof import('@/services/api')>()
  return {
    ...actual,
    api: Object.fromEntries(
      Object.keys(actual.api).map((name) => [name, vi.fn()])
    ),
  }
})
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: vi.fn() }))
enableAutoUnmount(afterEach)
const task = (id = 'a', title = '冻结标题'): Task =>
  ({
    id,
    title,
    status: 'open',
    priority: 1,
    hidden: false,
    archived: false,
    deletedAt: null,
    department: '网络部',
    recordedAt: 100,
    updateCount: 1,
  }) as Task
const update = (id: string, content: string, time = 150): TaskUpdate => ({
  id,
  taskId: 'a',
  content,
  author: '甲',
  kind: 'progress',
  createdAt: time,
  updatedAt: time,
  editedAt: null,
})
function record(finalized = false): HandoverDetail {
  return {
    handover: {
      id: 'h',
      title: '交接',
      handoverDate: '2026-11-06',
      periodStart: 100,
      periodEnd: 200,
      status: finalized ? 'finalized' : 'draft',
      shiftFrom: null,
      shiftTo: null,
      note: '保留整体备注',
      finalizedAt: finalized ? 200 : null,
      snapshotVersion: 1,
      createdAt: 100,
      updatedAt: 100,
    },
    items: [
      {
        id: 'item',
        taskId: 'a',
        section: 'pending',
        note: '保留条目备注',
        sortOrder: 0,
        selectedUpdateIds: ['u'],
        snapshot: { task: task(), updates: [update('u', '冻结进展')] },
      },
    ],
  }
}
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((yes) => {
    resolve = yes
  })
  return { promise, resolve }
}
beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.mocked(api.getHandover).mockResolvedValue(record())
  vi.mocked(api.saveHandover).mockResolvedValue(record())
  vi.mocked(api.finalizeHandover).mockResolvedValue(record(true))
})

it('a live progress request started in draft cannot replace the finalized frozen snapshot', async () => {
  const store = useHandoverStore()
  await store.open('h')
  const pending = deferred<TaskSnapshot>()
  vi.mocked(api.getTask).mockReturnValueOnce(pending.promise)
  const load = store.loadItemUpdates('a')
  await store.finalize()
  pending.resolve({
    task: task('a', '实时被改的标题'),
    updates: [
      update('u', '实时被改的进展'),
      update('excluded', '从未选中的实时进展'),
    ],
  })
  await load
  expect(store.finalized).toBe(true)
  expect(store.snapshots.a?.task.title).toBe('冻结标题')
  expect(store.snapshots.a?.updates.map((item) => item.content)).toEqual([
    '冻结进展',
  ])
})

it('a stale live progress failure cannot report an error after successful finalization', async () => {
  const store = useHandoverStore()
  await store.open('h')
  let reject!: (error: Error) => void
  vi.mocked(api.getTask).mockReturnValueOnce(
    new Promise((_resolve, no) => {
      reject = no
    })
  )
  const load = store.loadItemUpdates('a')
  await store.finalize()
  reject(new Error('过期的实时读取失败'))
  await load
  expect(store.finalized).toBe(true)
  expect(store.error).toBeNull()
})

it('leaving the editor invalidates an outstanding open response', async () => {
  const store = useHandoverStore()
  const pending = deferred<HandoverDetail>()
  vi.mocked(api.getHandover).mockReturnValueOnce(pending.promise)
  const open = store.open('h')
  store.invalidateEditorReads()
  pending.resolve(record())
  await open
  expect(store.detail).toBeNull()
  expect(store.editorLoading).toBe(false)
})

it('a pending candidate cannot be appended to a frozen handover after finalization', async () => {
  const store = useHandoverStore()
  await store.open('h')
  const pending = deferred<TaskSnapshot>()
  vi.mocked(api.getTask).mockReturnValueOnce(pending.promise)
  const add = store.addCandidate(task('b', '尚未加入的事项'))
  await store.finalize()
  pending.resolve({ task: task('b', '尚未加入的事项'), updates: [] })
  await add
  // A finalize may be deferred/refused until addition finishes, or may freeze the saved items.
  // In either case a finalized UI must exactly match its persisted frozen record.
  if (store.finalized)
    expect(store.form.items.map((item) => item.taskId)).toEqual(
      store.detail!.items.map((item) => item.taskId ?? item.snapshot.task.id)
    )
  else expect(api.finalizeHandover).not.toHaveBeenCalled()
})

it('switching records cancels the old candidate loading marker so that task can be added again', async () => {
  const store = useHandoverStore()
  await store.startDraft()
  const pending = deferred<TaskSnapshot>()
  vi.mocked(api.getTask).mockReturnValueOnce(pending.promise)
  const add = store.addCandidate(task('b'))
  await store.open('h')
  pending.resolve({ task: task('b'), updates: [] })
  await add
  expect(store.addingIds.b).not.toBe(true)
  vi.mocked(api.getTask).mockResolvedValue({ task: task('b'), updates: [] })
  await store.addCandidate(task('b'))
  expect(store.form.items.some((item) => item.taskId === 'b')).toBe(true)
})

it('the UI can remove a deleted or out-of-period update selection without losing item or overall notes', async () => {
  const store = useHandoverStore()
  await store.open('h')
  store.form.items[0].selectedUpdateIds = ['u', 'deleted', 'outside']
  store.availableUpdates.a = [
    update('u', '有效进展'),
    update('outside', '跨班进展', 200),
  ]
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div />' } }],
  })
  await router.push('/')
  const wrapper = mount(HandoverItems, { global: { plugins: [router] } })
  expect(wrapper.text()).toContain('有 2 条已选进展被删除或已超出当前班次')
  await wrapper
    .findAll('button')
    .find((item) => item.text() === '移除失效选择')!
    .trigger('click')
  expect(store.form.items[0].selectedUpdateIds).toEqual(['u'])
  expect(store.form.items[0].note).toBe('保留条目备注')
  expect(store.form.note).toBe('保留整体备注')
  wrapper.unmount()
})

it('an orphaned draft item offers removal while finalized orphan content remains visible and immutable', async () => {
  const orphan = record()
  orphan.items[0].taskId = null
  vi.mocked(api.getHandover).mockResolvedValue(orphan)
  const store = useHandoverStore()
  await store.open('h')
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: { template: '<div />' } }],
  })
  await router.push('/')
  const wrapper = mount(HandoverItems, { global: { plugins: [router] } })
  expect(wrapper.text()).toContain('请移除该条目后再保存或定稿')
  expect(
    wrapper.findAll('button').some((item) => item.text() === '查看事项')
  ).toBe(false)
  await wrapper
    .findAll('button')
    .find((item) => item.text() === '移除')!
    .trigger('click')
  expect(store.form.items).toEqual([])
  vi.mocked(api.getHandover).mockResolvedValue({
    ...orphan,
    handover: { ...orphan.handover, status: 'finalized', finalizedAt: 200 },
  })
  await store.open('h')
  await flushPromises()
  expect(wrapper.text()).toContain('定稿快照不受影响')
  expect(wrapper.text()).toContain('冻结进展')
  expect(wrapper.findAll('button').some((item) => item.text() === '移除')).toBe(
    false
  )
  expect(wrapper.get('textarea').element.matches(':disabled')).toBe(true)
  wrapper.unmount()
})
