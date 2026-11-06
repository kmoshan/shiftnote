import { expect, test, type Page } from '@playwright/test'

/** Only this browser test supplies IPC; production never bypasses the vault. */
async function installLockedIPC(page: Page, legacy = false, firstRun = false) {
  await page.addInitScript(({ legacy, firstRun }) => {
    const commands: string[] = []
    let unlocked = firstRun && localStorage.getItem('__testSetupSkipped') === '1'
    ;(window as any).__vaultTestCommands = commands
    ;(window as any).__TAURI_INTERNALS__ = {
      async invoke(command: string, args: any = {}) {
        commands.push(command)
        if (command === 'vault_status')
          return { configured: !legacy && !firstRun, unlocked, legacy: legacy && !unlocked }
        if (command === 'vault_skip') {
          if (!legacy && !firstRun) throw new Error('已加密数据不允许跳过')
          localStorage.setItem('__testSetupSkipped', '1')
          unlocked = true
          return
        }
        if (command === 'vault_unlock') {
          if (args.password === 'wrong')
            throw { code: 'vault_password', message: '口令错误，请重试' }
          unlocked = true
          return
        }
        if (!unlocked) throw new Error('业务数据在解锁前被请求')
        if (command === 'settings_get') return null
        if (command === 'task_page') return { items: [], total: 0, limit: 50, offset: 0 }
        if (command === 'task_counts')
          return { all: 0, today: 0, open: 0, focus: 0, done: 0, hidden: 0, trash: 0, archived: 0 }
        throw new Error(`Unexpected test IPC: ${command}`)
      },
    }
  }, { legacy, firstRun })
}

test('locked startup blocks business requests and clears failed password submissions', async ({ page }) => {
  await installLockedIPC(page)
  await page.goto('/')
  await expect(page.getByRole('heading', { name: '解锁本地数据' })).toBeVisible()
  await page.keyboard.press('Control+n')
  expect(await page.evaluate(() => (window as any).__vaultTestCommands)).toEqual(['vault_status'])
  await page.screenshot({ path: 'test-results/vault-locked-1180.png', fullPage: true })
  await page.getByLabel('加密口令', { exact: true }).fill('wrong')
  await page.getByRole('button', { name: '解锁', exact: true }).click()
  await expect(page.getByRole('alert')).toContainText('口令错误')
  await expect(page.getByLabel('加密口令', { exact: true })).toHaveValue('')
  expect(await page.evaluate(() => [localStorage.length, sessionStorage.length])).toEqual([0, 0])
  expect(await page.evaluate(() => (window as any).__vaultTestCommands)).toEqual(['vault_status', 'vault_unlock'])
  await page.getByLabel('加密口令', { exact: true }).fill('fixture-only-password')
  await page.getByRole('button', { name: '解锁', exact: true }).click()
  await expect(page.getByRole('heading', { name: '全部事项', exact: true, level: 1 })).toBeVisible()
  expect(await page.evaluate(() => (window as any).__vaultTestCommands)).toContain('task_page')
})

test('legacy setup fits the minimum window and explains migration and lost-password recovery', async ({ page }) => {
  await installLockedIPC(page, true)
  await page.setViewportSize({ width: 940, height: 620 })
  await page.goto('/')
  await expect(page.getByRole('heading', { name: '设置数据加密口令' })).toBeVisible()
  await expect(page.getByText('ShiftNote 不提供口令找回。忘记口令将无法恢复数据；在其他电脑恢复加密备份时，也需要原备份的口令。')).toBeVisible()
  await expect(page.getByRole('checkbox')).toBeVisible()
  const button = page.getByRole('button', { name: '设置口令并解锁' })
  await expect(button).toBeVisible()
  const box = (await button.boundingBox())!
  expect(box.y + box.height).toBeLessThanOrEqual(620)
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
  await page.screenshot({ path: 'test-results/vault-setup-940.png', fullPage: true })
})

test('first launch can skip encryption and subsequent launches enter directly', async ({ page }) => {
  await installLockedIPC(page, false, true)
  await page.setViewportSize({ width: 940, height: 620 })
  await page.goto('/')
  const skip = page.getByRole('button', { name: '暂不加密，直接进入' })
  await expect(skip).toBeVisible()
  await skip.click()
  await expect(page.getByRole('heading', { name: '全部事项', exact: true, level: 1 })).toBeVisible()
  expect(await page.evaluate(() => (window as any).__vaultTestCommands)).toContain('vault_skip')
  await page.reload()
  await expect(page.getByRole('heading', { name: '全部事项', exact: true, level: 1 })).toBeVisible()
  expect(await page.evaluate(() => (window as any).__vaultTestCommands)).not.toContain('vault_skip')
  expect(await page.evaluate(() => (window as any).__vaultTestCommands)).not.toContain('vault_unlock')
  await page.screenshot({ path: 'test-results/vault-skipped-940.png', fullPage: true })
})
