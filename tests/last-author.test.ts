import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'

import ProgressTimeline from '@/components/ProgressTimeline.vue'
import { api, ApiFailure } from '@/services/api'
import { useTaskStore } from '@/stores/tasks'
import { useVaultStore } from '@/stores/vault'
import type { Task, TaskUpdate } from '@/types'

vi.mock('@/services/api', async (load) => {
  const actual = await load<typeof import('@/services/api')>()
  return {
    ...actual,
    api: Object.fromEntries(Object.keys(actual.api).map((name) => [name, vi.fn()])),
  }
})
enableAutoUnmount(afterEach)

const legacyKey = 'shiftnote:last-author'
beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  localStorage.clear()
  vi.mocked(api.settingsGet).mockResolvedValue(null)
  vi.mocked(api.settingsSet).mockResolvedValue(undefined)
  vi.mocked(api.pageTasks).mockResolvedValue({ items: [], total: 0, limit: 50, offset: 0 })
  vi.mocked(api.counts).mockResolvedValue({ all: 0, today: 0, open: 0, focus: 0, done: 0, hidden: 0, trash: 0, archived: 0 })
  vi.mocked(api.listUpdates).mockResolvedValue([])
})
afterEach(() => vi.restoreAllMocks())

it('reads and migrates the old author only after unlock and removes plaintext only after encrypted save', async () => {
  localStorage.setItem(legacyKey, '原处理人')
  const readStorage = vi.spyOn(Storage.prototype, 'getItem')
  const writeStorage = vi.spyOn(Storage.prototype, 'setItem')
  const store = useTaskStore()
  await store.loadLastAuthor()
  expect(readStorage).not.toHaveBeenCalled()
  expect(api.settingsGet).not.toHaveBeenCalled()
  expect(store.getProgressDraft('before-unlock').author).toBe('')

  let finish!: () => void
  vi.mocked(api.settingsSet).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve }))
  useVaultStore().unlocked = true
  const loading = store.loadLastAuthor()
  await flushPromises()
  expect(api.settingsSet).toHaveBeenCalledWith('last_author', '原处理人')
  expect(localStorage.getItem(legacyKey)).toBe('原处理人')
  finish()
  await loading
  expect(localStorage.getItem(legacyKey)).toBeNull()
  expect(store.lastAuthor).toBe('原处理人')
  expect(store.getProgressDraft('before-unlock').author).toBe('原处理人')
  expect(writeStorage).not.toHaveBeenCalled()
})

it('keeps legacy plaintext on migration failure and preserves author input during a retry', async () => {
  localStorage.setItem(legacyKey, '旧处理人')
  useVaultStore().unlocked = true
  vi.mocked(api.settingsSet).mockRejectedValueOnce(new ApiFailure('设置暂时无法保存'))
  const store = useTaskStore()
  await store.loadLastAuthor()
  expect(localStorage.getItem(legacyKey)).toBe('旧处理人')
  expect(store.getProgressDraft('a').author).toBe('旧处理人')
  store.setProgressAuthor('a', '正在输入的人名')
  await store.loadLastAuthor()
  expect(localStorage.getItem(legacyKey)).toBeNull()
  expect(store.getProgressDraft('a').author).toBe('正在输入的人名')
})

it('saves a progress author through encrypted settings and uses it for new drafts', async () => {
  useVaultStore().unlocked = true
  const created = { id: 'progress', taskId: 'a', content: '已处理', author: '新处理人' } as TaskUpdate
  vi.mocked(api.addUpdate).mockResolvedValue(created)
  const writeStorage = vi.spyOn(Storage.prototype, 'setItem')
  const wrapper = mount(ProgressTimeline, { props: { taskId: 'a', updates: [] } })
  await wrapper.get('textarea').setValue('已处理')
  await wrapper.get('input[placeholder="处理人（选填）"]').setValue(' 新处理人 ')
  await wrapper.get('textarea').trigger('keydown', { key: 'Enter', ctrlKey: true })
  await flushPromises()
  expect(api.addUpdate).toHaveBeenCalledWith('a', '已处理', '新处理人')
  expect(api.settingsSet).toHaveBeenCalledWith('last_author', '新处理人')
  expect(useTaskStore().lastAuthor).toBe('新处理人')
  expect(useTaskStore().getProgressDraft('next').author).toBe('新处理人')
  expect(wrapper.get('textarea').element.value).toBe('')
  expect(writeStorage).not.toHaveBeenCalled()
})

it('fills a copied task author from the encrypted preference', async () => {
  useVaultStore().unlocked = true
  vi.mocked(api.settingsGet).mockResolvedValue('加密保存的处理人')
  const store = useTaskStore()
  await store.loadLastAuthor()
  const source = { id: 'source', title: '原事项' } as Task
  const copy = { id: 'copy', title: '原事项（副本）' } as Task
  vi.mocked(api.duplicateTask).mockResolvedValue(copy)
  vi.mocked(api.getTask).mockResolvedValue({ task: copy, updates: [] })
  await store.duplicateTask(source)
  const wrapper = mount(ProgressTimeline, { props: { taskId: store.selectedId!, updates: [] } })
  expect(wrapper.get('input[placeholder="处理人（选填）"]').element.value).toBe('加密保存的处理人')
  expect(localStorage.getItem(legacyKey)).toBeNull()
})

it('does not overwrite a manual author while the encrypted preference is loading', async () => {
  useVaultStore().unlocked = true
  let finish!: (value: string) => void
  vi.mocked(api.settingsGet).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve }))
  const store = useTaskStore()
  const loading = store.loadLastAuthor()
  store.getProgressDraft('a')
  store.setProgressAuthor('a', '')
  finish('已保存的人名')
  await loading
  expect(store.getProgressDraft('a').author).toBe('')
  expect(store.getProgressDraft('b').author).toBe('已保存的人名')
})
