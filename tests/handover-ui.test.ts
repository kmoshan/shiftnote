import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import {
  enableAutoUnmount,
  flushPromises,
  mount,
  type VueWrapper,
} from '@vue/test-utils'
import { createMemoryHistory, createRouter } from 'vue-router'
import { save } from '@tauri-apps/plugin-dialog'
import HandoverEditorView from '@/views/HandoverEditorView.vue'
import HandoverListView from '@/views/HandoverListView.vue'
import HandoverCandidates from '@/components/handover/HandoverCandidates.vue'
import { api, ApiFailure } from '@/services/api'
import { useConfirmStore } from '@/stores/confirm'
import { useTaskStore } from '@/stores/tasks'
import { useHandoverStore } from '@/stores/handovers'
import type { HandoverDetail, Task, TaskSnapshot } from '@/types'

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

const from = new Date(2026, 10, 6, 8).getTime()
const to = new Date(2026, 10, 6, 16).getTime()
function task(id = 'a'): Task {
  return {
    id,
    title: `事项${id}`,
    description: '事项说明',
    priority: 1,
    status: 'open',
    completedAt: null,
    hidden: false,
    archived: false,
    deletedAt: null,
    categoryId: null,
    department: '网络部',
    recordedAt: from,
    sortOrder: 1,
    createdAt: from,
    updatedAt: from,
    latestUpdateContent: null,
    latestUpdateAt: null,
    updateCount: 0,
    reminderEnabled: false,
    nextRemindAt: null,
  }
}
const snapshot = (id = 'a'): TaskSnapshot => ({ task: task(id), updates: [] })
function record(id = 'a', finalized = false): HandoverDetail {
  return {
    handover: {
      id,
      title: `已保存的交接${id}`,
      handoverDate: '2026-11-06',
      periodStart: from,
      periodEnd: to,
      status: finalized ? 'finalized' : 'draft',
      shiftFrom: '甲',
      shiftTo: '乙',
      note: '已保存备注',
      finalizedAt: finalized ? to : null,
      snapshotVersion: 1,
      createdAt: from,
      updatedAt: from,
    },
    items: [],
  }
}
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((yes) => {
    resolve = yes
  })
  return { promise, resolve }
}
function button(wrapper: VueWrapper, text: string) {
  const result = wrapper.findAll('button').find((item) => item.text() === text)
  if (!result) throw new Error(`缺少按钮：${text}`)
  return result
}
async function screen(path = '/handovers/new') {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<p>事项列表</p>' } },
      { path: '/handovers', component: HandoverListView },
      { path: '/handovers/new', component: HandoverEditorView },
      { path: '/handovers/:id', component: HandoverEditorView },
    ],
  })
  await router.push(path)
  const wrapper = mount(
    { template: '<router-view />' },
    { global: { plugins: [router] } }
  )
  await flushPromises()
  return { router, wrapper, store: useHandoverStore() }
}
beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.mocked(api.getHandover).mockImplementation(async (id) => record(id))
  vi.mocked(api.getTask).mockImplementation(async (id) => snapshot(id))
  vi.mocked(api.listHandovers).mockResolvedValue({
    items: [record().handover],
    total: 1,
    limit: 50,
    offset: 0,
  })
  vi.mocked(api.handoverCandidates).mockResolvedValue({
    items: [],
    total: 0,
    limit: 50,
    offset: 0,
  })
  vi.mocked(api.renderHandover).mockResolvedValue('冻结的导出正文')
})

it('new-draft query navigation honours Continue editing and keeps notes and chosen tasks', async () => {
  const { wrapper, router, store } = await screen('/handovers/new?taskIds=a')
  await wrapper
    .get('textarea[placeholder="当班概况、注意事项与交接说明"]')
    .setValue('必须保留的未保存说明')
  const ask = vi.spyOn(useConfirmStore(), 'ask').mockResolvedValue(false)
  await router.push('/handovers/new?taskIds=b')
  await flushPromises()
  expect(router.currentRoute.value.query.taskIds).toBe('a')
  expect(store.form.note).toBe('必须保留的未保存说明')
  expect(store.form.items.map((item) => item.taskId)).toEqual(['a'])
  expect(ask).toHaveBeenCalledOnce()
  wrapper.unmount()
})

it('discarding an existing draft and reopening it reloads saved content', async () => {
  const { wrapper, router } = await screen('/handovers/a')
  await wrapper.get('input[maxlength="200"]').setValue('应当放弃的修改')
  vi.spyOn(useConfirmStore(), 'ask').mockResolvedValue(true)
  await router.push('/handovers')
  await flushPromises()
  await wrapper.get('a[href="/handovers/a"]').trigger('click')
  await flushPromises()
  expect(
    (wrapper.get('input[maxlength="200"]').element as HTMLInputElement).value
  ).toBe('已保存的交接a')
  expect(api.getHandover).toHaveBeenCalledTimes(2)
  wrapper.unmount()
})

it('cancelling View task navigation does not open an unrelated task detail beside the draft', async () => {
  const { wrapper, router } = await screen('/handovers/new?taskIds=a')
  vi.spyOn(useConfirmStore(), 'ask').mockResolvedValue(false)
  const select = vi
    .spyOn(useTaskStore(), 'selectTask')
    .mockResolvedValue(undefined)
  await button(wrapper, '查看事项').trigger('click')
  await flushPromises()
  expect(router.currentRoute.value.path).toBe('/handovers/new')
  expect(select).not.toHaveBeenCalled()
  wrapper.unmount()
})

it('saving and finalizing stay disabled until the clicked candidate has actually been added', async () => {
  vi.mocked(api.handoverCandidates).mockResolvedValue({
    items: [task()],
    total: 1,
    limit: 50,
    offset: 0,
  })
  const pending = deferred<TaskSnapshot>()
  vi.mocked(api.getTask).mockReturnValueOnce(pending.promise)
  const { wrapper } = await screen()
  await wrapper
    .getComponent(HandoverCandidates)
    .findAll('button')
    .find((item) => item.text().includes('事项a'))!
    .trigger('click')
  const saveDisabled = button(wrapper, '保存草稿').element.matches(':disabled')
  const finalizeDisabled = button(wrapper, '定稿并冻结').element.matches(
    ':disabled'
  )
  pending.resolve(snapshot())
  await flushPromises()
  expect(saveDisabled).toBe(true)
  expect(finalizeDisabled).toBe(true)
  wrapper.unmount()
})

it('a new record can preview after leaving another record whose preview is still pending', async () => {
  const pending = deferred<string>()
  vi.mocked(api.renderHandover).mockReturnValueOnce(pending.promise)
  const { wrapper, router } = await screen('/handovers/a')
  await button(wrapper, '预览').trigger('click')
  await router.push('/handovers/b')
  await flushPromises()
  const previewButton = wrapper
    .findAll('header button')
    .find((item) => /预览|生成中/.test(item.text()))!
  const blocked = previewButton.element.matches(':disabled')
  pending.resolve('a 的旧预览')
  await flushPromises()
  expect(blocked).toBe(false)
  expect(wrapper.text()).not.toContain('a 的旧预览')
  wrapper.unmount()
})

it('cancelling finalization keeps the unsaved form and makes no save or finalize request', async () => {
  const { wrapper, store } = await screen()
  await wrapper.get('input[maxlength="200"]').setValue('尚需核对的夜班')
  vi.spyOn(useConfirmStore(), 'ask').mockResolvedValue(false)
  await button(wrapper, '定稿并冻结').trigger('click')
  await flushPromises()
  expect(api.saveHandover).not.toHaveBeenCalled()
  expect(api.finalizeHandover).not.toHaveBeenCalled()
  expect(store.form.title).toBe('尚需核对的夜班')
  wrapper.unmount()
})

it('cancelled export and declined overwrite retain the same frozen record and do not overwrite a file', async () => {
  vi.mocked(api.getHandover).mockResolvedValue(record('a', true))
  const { wrapper, store } = await screen('/handovers/a')
  expect(wrapper.findComponent(HandoverCandidates).exists()).toBe(false)
  expect(
    wrapper.get('input[maxlength="200"]').element.matches(':disabled')
  ).toBe(true)
  vi.mocked(save)
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce('C:\\夜班.md')
  await button(wrapper, 'Markdown').trigger('click')
  await flushPromises()
  expect(wrapper.text()).toContain('Markdown / TXT 导出为未加密文件')
  expect(save).toHaveBeenCalledWith(expect.objectContaining({
    title: '导出明文交接记录（不受口令保护）',
  }))
  expect(api.exportHandover).not.toHaveBeenCalled()
  vi.mocked(api.exportHandover).mockRejectedValueOnce(
    new ApiFailure('目标已存在', 'conflict')
  )
  vi.spyOn(useConfirmStore(), 'ask').mockResolvedValue(false)
  await button(wrapper, 'Markdown').trigger('click')
  await flushPromises()
  expect(api.exportHandover).toHaveBeenCalledExactlyOnceWith(
    'a',
    'markdown',
    'C:\\夜班.md',
    false
  )
  expect(store.finalized).toBe(true)
  expect(store.error).toBeNull()
  expect(store.busy).toBe(false)
  wrapper.unmount()
})

it('a failed overwrite stays retryable and leaves the frozen content intact', async () => {
  vi.mocked(api.getHandover).mockResolvedValue(record('a', true))
  const { wrapper, store } = await screen('/handovers/a')
  vi.mocked(save).mockResolvedValue('C:\\夜班.txt')
  vi.mocked(api.exportHandover)
    .mockRejectedValueOnce(new ApiFailure('目标已存在', 'conflict'))
    .mockRejectedValueOnce(new ApiFailure('文件被占用', 'io'))
  vi.spyOn(useConfirmStore(), 'ask').mockResolvedValue(true)
  await button(wrapper, 'TXT').trigger('click')
  await flushPromises()
  expect(api.exportHandover).toHaveBeenLastCalledWith(
    'a',
    'txt',
    'C:\\夜班.txt',
    true
  )
  expect(wrapper.get('[role="alert"]').text()).toContain('文件被占用')
  expect(store.finalized).toBe(true)
  expect(button(wrapper, 'TXT').element.matches(':disabled')).toBe(false)
  vi.mocked(api.exportHandover).mockResolvedValueOnce({
    path: 'C:\\夜班.txt',
    bytes: 10,
  })
  await button(wrapper, 'TXT').trigger('click')
  await flushPromises()
  expect(store.error).toBeNull()
  expect(store.form.title).toBe('已保存的交接a')
  wrapper.unmount()
})
