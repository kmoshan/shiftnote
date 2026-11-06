import { expect, test } from '@playwright/test'

test('checks after unlock and exposes the release page through banner and settings', async ({ page }) => {
  await page.addInitScript(() => {
    let unlocked = false
    const calls: { command: string; args: any }[] = []
    ;(window as any).__updateCalls = calls
    ;(window as any).__TAURI_INTERNALS__ = {
      async invoke(command: string, args: any = {}) {
        calls.push({ command, args })
        if (command === 'vault_status') return { configured: true, unlocked, legacy: false }
        if (command === 'vault_unlock') { unlocked = true; return }
        if (!unlocked) throw new Error('解锁前不应访问业务或更新配置')
        if (command === 'settings_get') return null
        if (command === 'settings_set') return
        if (command === 'task_page') return { items: [], total: 0, limit: 50, offset: 0 }
        if (command === 'task_counts') return { all: 0, today: 0, open: 0, focus: 0, done: 0, hidden: 0, trash: 0, archived: 0 }
        if (command === 'check_update') return {
          currentVersion: '0.2.1', latestVersion: '0.2.2', hasUpdate: true,
          releaseName: '更新', releaseUrl: 'https://github.com/kmoshan/shiftnote/releases/tag/v0.2.2',
          publishedAt: null, notes: '更新测试：保留口令与事项', checkedAt: Date.now(), usedProxy: false,
        }
        if (command === 'app_info') return { version: '0.2.1', dataDir: 'C:\\fixture', dbPath: 'C:\\fixture\\shiftnote.db', schemaVersion: 3, recoveredBackup: null, encrypted: true, plaintextBackupCount: 0 }
        if (command === 'db_stats') return { path: 'C:\\fixture\\shiftnote.db', sizeBytes: 100, taskCount: 0, updateCount: 0, schemaVersion: 3, walEnabled: false }
        if (command === 'plugin:opener|open_url') return
        throw new Error(`Unexpected test IPC: ${command}`)
      },
    }
  })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: '解锁本地数据' })).toBeVisible()
  expect(await page.evaluate(() => (window as any).__updateCalls.map((call: any) => call.command))).toEqual(['vault_status'])
  await page.getByLabel('加密口令', { exact: true }).fill('fixture-only-password')
  await page.getByRole('button', { name: '解锁', exact: true }).click()
  await expect(page.getByRole('button', { name: '查看更新' })).toBeVisible()
  await page.getByRole('button', { name: '查看更新' }).click()
  expect(await page.evaluate(() => (window as any).__updateCalls.find((call: any) => call.command === 'plugin:opener|open_url').args.url))
    .toBe('https://github.com/kmoshan/shiftnote/releases/tag/v0.2.2')
  await page.getByRole('link', { name: '设置', exact: true }).click()
  await page.getByRole('button', { name: '检测更新', exact: true }).click()
  await expect(page.getByText('更新测试：保留口令与事项')).toBeVisible()
  await expect(page.getByRole('button', { name: '下载 v0.2.2' })).toBeVisible()
  expect(await page.evaluate(() => (window as any).__updateCalls.filter((call: any) => call.command === 'check_update').length)).toBe(2)
  await page.screenshot({ path: 'test-results/update-settings.png', fullPage: true })
})
