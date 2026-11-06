import { test, expect, type Page } from '@playwright/test'

/** Browser interaction fixture only. Rust business behavior is verified separately. */
async function installIPC(page: Page, amount = 1001) {
  await page.addInitScript(
    ({ amount }) => {
      const now = Date.now()
      const tasks = Array.from({ length: amount }, (_, index) => ({
        id: `task-${index + 1}`,
        title: `值班事项 ${String(index + 1).padStart(4, '0')}`,
        description: '数据库切换与中文记录',
        priority: index % 4,
        status: 'open',
        completedAt: null,
        hidden: false,
        archived: false,
        deletedAt: null,
        categoryId: null,
        department: '运行维护部',
        recordedAt: now,
        sortOrder: index * 10,
        createdAt: now,
        updatedAt: now,
        latestUpdateContent:
          index === 0 ? '确认 #切换 [完成]，下一班继续观察' : null,
        latestUpdateAt: index === 0 ? now : null,
        updateCount: index === 0 ? 1 : 0,
        reminderEnabled: false,
        nextRemindAt: null,
      }))
      const updates = [
        {
          id: 'update-1',
          taskId: 'task-1',
          content: '确认 #切换 [完成]，下一班继续观察',
          author: '张三',
          kind: 'progress',
          createdAt: now,
          updatedAt: now,
          editedAt: null,
        },
      ]
      const records: Record<string, any> = {}
      const state = {
        calls: [] as { command: string; args: any }[],
        exportPath: null as string | null,
        conflict: false,
        ioFail: false,
        exportCount: 0,
      }
      ;(window as any).__testIPC = state
      const clone = (value: any) => structuredClone(value)
      const snapshot = (item: any) => ({
        task: clone(tasks.find((task) => task.id === item.taskId)),
        updates: clone(
          updates.filter((update) => item.selectedUpdateIds.includes(update.id))
        ),
      })
      ;(window as any).__TAURI_INTERNALS__ = {
        async invoke(command: string, args: any = {}) {
          state.calls.push({ command, args: clone(args) })
          if (command === 'vault_status')
            return { configured: true, unlocked: true, legacy: false }
          if (command === 'settings_all') return { theme: 'light' }
          if (command === 'settings_get') return null
          if (command === 'settings_set') return null
          if (command === 'task_counts')
            return {
              all: tasks.length,
              today: tasks.length,
              open: tasks.filter((task) => task.status === 'open').length,
              focus: tasks.filter((task) => task.priority <= 1).length,
              done: tasks.filter((task) => task.status === 'done').length,
              hidden: 0,
              archived: 0,
              trash: 0,
            }
          if (command === 'task_page' || command === 'handover_candidates') {
            const f = args.filter
            let rows = tasks.filter(
              (task) => !task.hidden && !task.archived && !task.deletedAt
            )
            if (f.view === 'done')
              rows = rows.filter((task) => task.status === 'done')
            if (f.priority != null)
              rows = rows.filter((task) => task.priority === f.priority)
            if (f.search)
              rows = rows.filter((task) => task.title.includes(f.search))
            return clone({
              items: rows.slice(
                f.offset ?? 0,
                (f.offset ?? 0) + (f.limit ?? 50)
              ),
              total: rows.length,
              limit: f.limit ?? 50,
              offset: f.offset ?? 0,
            })
          }
          if (command === 'task_get')
            return clone({
              task: tasks.find((task) => task.id === args.id),
              updates: updates.filter((update) => update.taskId === args.id),
            })
          if (command === 'update_list')
            return clone(
              updates.filter((update) => update.taskId === args.taskId)
            )
          if (command === 'handover_list')
            return clone({
              items: Object.values(records).map((record) => record.handover),
              total: Object.keys(records).length,
              limit: 50,
              offset: 0,
            })
          if (command === 'handover_save') {
            const input = args.input
            const id = input.id ?? `handover-${Object.keys(records).length + 1}`
            records[id] = {
              handover: {
                id,
                ...input,
                handoverDate: '2026-11-06',
                status: 'draft',
                finalizedAt: null,
                snapshotVersion: 1,
                createdAt: now,
                updatedAt: now,
              },
              items: input.items.map((item: any, index: number) => ({
                ...item,
                id: `item-${index}`,
                sortOrder: index,
                snapshot: snapshot(item),
              })),
            }
            return clone(records[id])
          }
          if (command === 'handover_get') return clone(records[args.id])
          if (command === 'handover_finalize') {
            const record = records[args.id]
            record.handover.status = 'finalized'
            record.handover.finalizedAt = now
            return clone(record)
          }
          if (command === 'handover_render') {
            const record = records[args.id]
            return (
              `${record.handover.title}\r\n${record.handover.note ?? ''}\r\n` +
              record.items
                .map(
                  (item: any) =>
                    `${item.snapshot.task.title}\r\n${item.note ?? ''}\r\n${item.snapshot.updates.map((update: any) => update.content).join('\r\n')}`
                )
                .join('\r\n')
            )
          }
          if (command === 'plugin:dialog|save') return state.exportPath
          if (command === 'handover_export') {
            if (state.conflict && !args.overwrite)
              throw { code: 'conflict', message: '目标文件已存在' }
            if (state.ioFail) throw { code: 'io', message: '文件被占用' }
            ++state.exportCount
            return { path: args.path, bytes: 200 }
          }
          throw new Error(`Unexpected test IPC: ${command}`)
        },
      }
    },
    { amount }
  )
}

test('1001 records are pageable and page selection is separate from completion', async ({
  page,
}) => {
  await installIPC(page)
  await page.goto('/')
  await expect(
    page.getByText('共 1001 条', { exact: false }).first()
  ).toBeVisible()
  await page.getByRole('checkbox', { name: '选择事项：值班事项 0001' }).check()
  await expect(page.getByText('已选 1 条')).toBeVisible()
  await expect(
    page.getByRole('button', { name: '标记完成', exact: true }).first()
  ).toBeVisible()
  await page.getByRole('button', { name: '下一页', exact: true }).click()
  await expect(
    page.getByRole('checkbox', { name: '选择事项：值班事项 0051' })
  ).toBeVisible()
  await expect(page.getByText('已选 0 条')).toBeVisible()
  await page.getByRole('combobox', { name: '每页条数' }).selectOption('500')
  await page.getByRole('button', { name: '下一页', exact: true }).click()
  await page.getByRole('button', { name: '下一页', exact: true }).click()
  await expect(
    page.getByRole('checkbox', { name: '选择事项：值班事项 1001' })
  ).toBeVisible()
})

test('selected task to handover, preview, freeze and controlled file export', async ({
  page,
}) => {
  await installIPC(page)
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto('/')
  await page.getByRole('checkbox', { name: '选择事项：值班事项 0001' }).check()
  await page.getByRole('button', { name: '从已选事项发起交接' }).click()
  await expect(page.getByRole('heading', { name: /^交接事项/ })).toBeVisible()
  await page.getByLabel('交接标题').fill('中文夜班 #交接')
  await page.getByLabel('整体备注').fill('跨班观察，不丢历史。')
  await page.getByLabel('交接备注', { exact: true }).fill('下一班确认连接恢复')
  await page.getByRole('button', { name: '预览', exact: true }).click()
  await expect(page.locator('pre')).toContainText('中文夜班 #交接')
  await expect(page.locator('pre')).toContainText('确认 #切换 [完成]')
  await page.getByRole('button', { name: '定稿并冻结', exact: true }).click()
  await page.getByRole('button', { name: '保存并定稿', exact: true }).click()
  await expect(page.getByRole('heading', { name: /交接历史/ })).toBeVisible()
  await expect(page.getByLabel('交接标题')).toBeDisabled()
  await page.getByRole('button', { name: 'TXT', exact: true }).first().click()
  expect(await page.evaluate(() => (window as any).__testIPC.exportCount)).toBe(
    0
  )
  await page.evaluate(() => {
    ;(window as any).__testIPC.exportPath = 'C:\\exports\\夜班.txt'
    ;(window as any).__testIPC.conflict = true
  })
  await page.getByRole('button', { name: 'TXT', exact: true }).first().click()
  await page.getByRole('button', { name: '取消', exact: true }).click()
  expect(await page.evaluate(() => (window as any).__testIPC.exportCount)).toBe(
    0
  )
  await page.getByRole('button', { name: 'TXT', exact: true }).first().click()
  await page.getByRole('button', { name: '覆盖文件', exact: true }).click()
  expect(await page.evaluate(() => (window as any).__testIPC.exportCount)).toBe(
    1
  )
  await page.evaluate(() => {
    ;(window as any).__testIPC.conflict = false
    ;(window as any).__testIPC.ioFail = true
  })
  await page.getByRole('button', { name: 'TXT', exact: true }).first().click()
  await expect(page.getByRole('alert')).toContainText('文件被占用')
  await page.screenshot({
    path: 'test-results/handover-finalized.png',
    fullPage: true,
  })
  expect(errors).toEqual([])
})

test('editor fits the minimum desktop window without horizontal overflow', async ({
  page,
}) => {
  await installIPC(page, 2)
  await page.setViewportSize({ width: 940, height: 620 })
  await page.goto('/#/handovers/new?taskIds=task-1')
  await expect(page.getByRole('heading', { name: /^交接事项/ })).toBeVisible()
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth
    )
  ).toBe(true)
  await page.screenshot({
    path: 'test-results/handover-940.png',
    fullPage: true,
  })
})
