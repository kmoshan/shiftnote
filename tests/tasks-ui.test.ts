import { beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { flushPromises, mount } from '@vue/test-utils'
import { createMemoryHistory, createRouter } from 'vue-router'
import TaskList from '@/components/TaskList.vue'
import TaskItem from '@/components/TaskItem.vue'
import { api } from '@/services/api'
import { useTaskStore } from '@/stores/tasks'
import type { Task } from '@/types'

vi.mock('@/services/api', async (original) => {
  const actual = await original<typeof import('@/services/api')>()
  return {
    ...actual,
    api: Object.fromEntries(
      [
        'pageTasks',
        'counts',
        'getTask',
        'listUpdates',
        'editUpdate',
        'moveTask',
        'setCompleted',
        'setArchived',
        'batchTasks',
      ].map((name) => [name, vi.fn()])
    ),
  }
})
const makeTask = (id: string): Task => ({
  id,
  title: `事项${id}`,
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
})
beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.mocked(api.counts).mockResolvedValue({
    all: 101,
    today: 0,
    open: 101,
    focus: 0,
    done: 0,
    hidden: 0,
    trash: 0,
    archived: 0,
  })
  vi.mocked(api.pageTasks).mockResolvedValue({
    items: [makeTask('a'), makeTask('b')],
    total: 101,
    limit: 50,
    offset: 0,
  })
})
async function list() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/handovers/new', component: { template: '<div />' } },
    ],
  })
  await router.push('/')
  const store = useTaskStore()
  await store.refresh()
  return {
    store,
    router,
    wrapper: mount(TaskList, { global: { plugins: [router] } }),
  }
}

it('T05 DOM selection never toggles completion and handover receives exactly the selected IDs', async () => {
  const { store, router, wrapper } = await list()
  expect(wrapper.text()).toContain('共 101 条 · 本页 2 条')
  await wrapper.get('[aria-label="选择事项：事项a"]').setValue(true)
  expect(store.selectedIds).toEqual(['a'])
  expect(api.setCompleted).not.toHaveBeenCalled()
  const handover = wrapper
    .findAll('button')
    .find((button) => button.text() === '从已选事项发起交接')!
  await handover.trigger('click')
  await flushPromises()
  expect(router.currentRoute.value.query.taskIds).toBe('a')
  wrapper.unmount()
})

it('T06 DOM range displays invalid input and does not request it; today explains its search scope', async () => {
  const { store, wrapper } = await list()
  await wrapper.get('[aria-label="记录开始日期"]').setValue('2026-12-31')
  await wrapper
    .get('[aria-label="记录结束日期（含当天）"]')
    .setValue('2026-12-30')
  await flushPromises()
  expect(wrapper.get('[role="alert"]').text()).toContain(
    '开始日期不能晚于结束日期'
  )
  expect(api.pageTasks).toHaveBeenCalledTimes(2)
  await store.setView('today')
  await flushPromises()
  expect(wrapper.find('[aria-label="记录开始日期"]').exists()).toBe(false)
  expect(wrapper.text()).toContain('搜索仍限今天')
  wrapper.unmount()
})

it('T10 DOM archive is offered only to completed tasks and manual dragging gives its reason', async () => {
  const { store, wrapper } = await list()
  const first = wrapper.findAllComponents(TaskItem)[0]
  await first.get('[title="更多操作"]').trigger('click')
  expect(
    first.findAll('button').some((button) => button.text() === '归档事项')
  ).toBe(false)
  store.tasks[0].status = 'done'
  await flushPromises()
  expect(
    first.findAll('button').some((button) => button.text() === '归档事项')
  ).toBe(true)
  await store.setSort('manual')
  await store.setPriorityFilter(0)
  await flushPromises()
  expect(wrapper.text()).toContain('清除搜索、优先级和记录日期条件')
  expect(
    first.get('[aria-label="拖动以手动排序"]').attributes('disabled')
  ).toBeDefined()
  wrapper.unmount()
})

it('T10 a rendered drag sends successor anchor rather than reordering this page', async () => {
  const { store, wrapper } = await list()
  await store.setSort('manual')
  await flushPromises()
  const rows = wrapper.findAllComponents(TaskItem)
  const transfer = {
    effectAllowed: '',
    dropEffect: '',
    setData: vi.fn(),
    setDragImage: vi.fn(),
  }
  await rows[1]
    .get('[aria-label="拖动以手动排序"]')
    .trigger('dragstart', { dataTransfer: transfer })
  await rows[0].trigger('dragover', { dataTransfer: transfer })
  await rows[0].trigger('drop', { dataTransfer: transfer })
  await flushPromises()
  expect(api.moveTask).toHaveBeenCalledWith({
    id: 'b',
    afterId: 'a',
    view: 'all',
  })
  wrapper.unmount()
})

it('T10 disables drag while another page is loading', async () => {
  const { store, wrapper } = await list()
  await store.setSort('manual')
  let finish!: (value: Awaited<ReturnType<typeof api.pageTasks>>) => void
  vi.mocked(api.pageTasks).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve
      })
  )
  const pending = store.setPage(2)
  await flushPromises()
  expect(
    wrapper.get('[aria-label="拖动以手动排序"]').attributes('disabled')
  ).toBeDefined()
  finish({ items: [makeTask('c')], total: 101, limit: 50, offset: 50 })
  await pending
  wrapper.unmount()
})
