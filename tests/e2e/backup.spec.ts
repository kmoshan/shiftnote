import { expect, test } from '@playwright/test'

for (const encrypted of [false, true]) {
  test(`settings backup and restore remain available (encrypted=${encrypted})`, async ({ page }) => {
    const pageErrors: string[] = []
    page.on('pageerror', error => pageErrors.push(error.message))
    await page.addInitScript(({ encrypted }) => {
      const state = { unlocked: !encrypted, picked: null as string | null, restored: false, restores: 0 }
      ;(window as any).__backupTest = state
      ;(window as any).__TAURI_INTERNALS__ = {
        async invoke(command: string, args: any = {}) {
          if (command === 'vault_status') return { configured: encrypted, unlocked: state.unlocked, legacy: false }
          if (command === 'vault_unlock') { state.unlocked = true; return }
          if (!state.unlocked) throw new Error('工作区尚未解锁')
          if (command === 'settings_get') return args.key === 'theme' && state.restored ? 'dark' : null
          if (command === 'settings_set') return
          if (command === 'check_update') return {
            currentVersion: '0.2.1', latestVersion: '0.2.0', hasUpdate: false,
            releaseName: null, releaseUrl: 'https://github.com/kmoshan/shiftnote/releases/tag/v0.2.0',
            publishedAt: null, notes: null, checkedAt: Date.now(), usedProxy: false,
          }
          if (command === 'task_page') return { items: [], total: 0, limit: 50, offset: 0 }
          if (command === 'task_counts') return { all: 0, today: 0, open: 0, focus: 0, done: 0, hidden: 0, trash: 0, archived: 0 }
          if (command === 'app_info') return {
            version: '0.2.1', dataDir: 'C:\\fixture', dbPath: 'C:\\fixture\\shiftnote.db',
            schemaVersion: 3, recoveredBackup: null, encrypted, plaintextBackupCount: 0,
          }
          if (command === 'db_stats') return {
            path: 'C:\\fixture\\shiftnote.db', sizeBytes: 1024, taskCount: 0, updateCount: 0, schemaVersion: 3, walEnabled: false,
          }
          if (command === 'db_backup') return {
            path: `C:\\fixture\\backups\\中文 备份.${encrypted ? 'snbak' : 'db'}`, sizeBytes: 1024, createdAt: Date.now(),
          }
          if (command === 'plugin:dialog|open') return state.picked
          if (command === 'db_restore') {
            ++state.restores
            if (args.backupPassword !== 'fixture-backup-password') throw { code: 'vault_password', message: '备份口令错误，请重试' }
            state.restored = true
            return {
              sourcePath: args.path, snapshotPath: `C:\\fixture\\backups\\pre-restore-test.${encrypted ? 'snbak' : 'db'}`,
              schemaVersion: 3, taskCount: 0, updateCount: 0,
            }
          }
          throw new Error(`Unexpected test IPC: ${command}`)
        },
      }
    }, { encrypted })
    await page.goto('/')
    if (encrypted) {
      await page.getByLabel('加密口令', { exact: true }).fill('fixture-current-password')
      await page.getByRole('button', { name: '解锁', exact: true }).click()
    }
    await page.getByRole('link', { name: '设置', exact: true }).click()
    const backup = page.getByRole('button', { name: '立即备份', exact: true })
    const restore = page.getByRole('button', { name: '从备份恢复', exact: true })
    await expect(backup).toBeVisible()
    await expect(restore).toBeVisible()
    await expect(page.getByText('事项总数', { exact: true })).toBeVisible()
    await backup.click()
    await expect(page.getByText(/最近一次备份：/)).toContainText(`中文 备份.${encrypted ? 'snbak' : 'db'}`)
    await restore.click()
    await expect(page.getByRole('dialog')).not.toBeVisible()
    expect(await page.evaluate(() => (window as any).__backupTest.restores)).toBe(0)
    await page.evaluate(() => { (window as any).__backupTest.picked = 'C:\\Backups\\其他电脑的备份.snbak' })
    await restore.click()
    await expect(page.getByRole('dialog')).toBeVisible()
    await page.getByLabel('原备份口令', { exact: true }).fill('incorrect-fixture-password')
    await page.getByRole('button', { name: '开始恢复', exact: true }).click()
    await expect(page.getByRole('alert')).toContainText('备份口令错误')
    await expect(page.getByLabel('原备份口令', { exact: true })).toHaveValue('')
    expect(await page.evaluate(() => (window as any).__backupTest.restored)).toBe(false)
    await page.getByLabel('原备份口令', { exact: true }).fill('fixture-backup-password')
    await page.getByRole('button', { name: '开始恢复', exact: true }).click()
    await expect(page.getByRole('dialog')).not.toBeVisible()
    await expect(page.getByText(/恢复前的数据已快照到：/)).toContainText(`pre-restore-test.${encrypted ? 'snbak' : 'db'}`)
    await expect(page.locator('html')).toHaveClass(/dark/)
    expect(await page.evaluate(() => (window as any).__backupTest.restores)).toBe(2)
    expect(pageErrors).toEqual([])
    await page.screenshot({ path: `test-results/backup-${encrypted ? 'encrypted' : 'plain'}.png`, fullPage: true })
  })
}
