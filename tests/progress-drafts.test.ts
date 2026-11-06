import { beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { flushPromises, mount } from '@vue/test-utils'
import ProgressTimeline from '@/components/ProgressTimeline.vue'
import { useTaskStore } from '@/stores/tasks'
import type { TaskUpdate } from '@/types'

beforeEach(() => {
  setActivePinia(createPinia())
  localStorage.clear()
})

it('T01 preserves separate task drafts through task changes and component remounts', async () => {
  const wrapper = mount(ProgressTimeline, {
    props: { taskId: 'a', updates: [] },
  })
  await wrapper.get('textarea').setValue('A 的草稿')
  await wrapper.setProps({ taskId: 'b' })
  expect(wrapper.get('textarea').element.value).toBe('')
  await wrapper.get('textarea').setValue('B 的草稿')
  wrapper.unmount()
  const reopened = mount(ProgressTimeline, {
    props: { taskId: 'a', updates: [] },
  })
  expect(reopened.get('textarea').element.value).toBe('A 的草稿')
  reopened.unmount()
})

it('T01 saving A after switching to B clears only the captured A draft', async () => {
  let resolve!: (value: TaskUpdate) => void
  vi.spyOn(useTaskStore(), 'addProgress').mockImplementation(
    () =>
      new Promise((yes) => {
        resolve = yes
      })
  )
  const wrapper = mount(ProgressTimeline, {
    props: { taskId: 'a', updates: [] },
  })
  await wrapper.get('textarea').setValue('A 的草稿')
  await wrapper
    .get('textarea')
    .trigger('keydown', { key: 'Enter', ctrlKey: true })
  await wrapper.setProps({ taskId: 'b' })
  await wrapper.get('textarea').setValue('B 的草稿')
  resolve({ id: 'created' } as TaskUpdate)
  await flushPromises()
  expect(wrapper.get('textarea').element.value).toBe('B 的草稿')
  await wrapper.setProps({ taskId: 'a' })
  expect(wrapper.get('textarea').element.value).toBe('')
  wrapper.unmount()
})

it('T01 isolates in-progress edits by task and does not cancel B when A finishes', async () => {
  const a = {
    id: 'a-update',
    taskId: 'a',
    content: 'A 原文',
    createdAt: 1,
  } as TaskUpdate
  const b = {
    id: 'b-update',
    taskId: 'b',
    content: 'B 原文',
    createdAt: 1,
  } as TaskUpdate
  let resolve!: (value: TaskUpdate) => void
  vi.spyOn(useTaskStore(), 'editProgress').mockImplementation(
    () =>
      new Promise((yes) => {
        resolve = yes
      })
  )
  const wrapper = mount(ProgressTimeline, {
    props: { taskId: 'a', updates: [a] },
  })
  await wrapper.get('[title="修改这条进展"]').trigger('click')
  await wrapper.get('#update-editor-a-update').setValue('A 修改')
  await wrapper
    .get('#update-editor-a-update')
    .trigger('keydown', { key: 'Enter', ctrlKey: true })
  await wrapper.setProps({ taskId: 'b', updates: [b] })
  await wrapper.get('[title="修改这条进展"]').trigger('click')
  await wrapper.get('#update-editor-b-update').setValue('B 修改')
  resolve({ ...a, content: 'A 修改' })
  await flushPromises()
  expect(wrapper.get('#update-editor-b-update').element.value).toBe('B 修改')
  wrapper.unmount()
})
