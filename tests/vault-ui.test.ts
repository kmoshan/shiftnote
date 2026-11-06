import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils'
import { createMemoryHistory, createRouter } from 'vue-router'
import { open } from '@tauri-apps/plugin-dialog'

import App from '@/App.vue'
import VaultUnlock from '@/components/VaultUnlock.vue'
import SettingsView from '@/views/SettingsView.vue'
import { api, ApiFailure } from '@/services/api'
import { useTaskStore } from '@/stores/tasks'
import { useUiStore } from '@/stores/ui'
import { useVaultStore } from '@/stores/vault'
import { useToastStore } from '@/stores/toast'
import type { AppInfo } from '@/types'

vi.mock('@/services/api', async (load) => {
  const actual = await load<typeof import('@/services/api')>()
  return {
    ...actual,
    isTauriRuntime: () => true,
    api: Object.fromEntries(
      Object.keys(actual.api).map((name) => [name, vi.fn()])
    ),
  }
})
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
enableAutoUnmount(afterEach)

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.mocked(api.vaultStatus).mockResolvedValue({
    configured: true,
    unlocked: false,
    legacy: false,
  })
  vi.mocked(api.vaultUnlock).mockResolvedValue(undefined)
})
afterEach(() => {
  vi.restoreAllMocks()
  document.body.innerHTML = ''
})

async function unlockScreen(configured = true, legacy = false) {
  vi.mocked(api.vaultStatus).mockResolvedValue({
    configured,
    unlocked: false,
    legacy,
  })
  await useVaultStore().initialize()
  return mount(VaultUnlock)
}

function settingsInfo(plaintextBackupCount = 0): AppInfo {
  return {
    version: '0.1.0',
    dataDir: 'C:\\ShiftNote',
    dbPath: 'C:\\ShiftNote\\shiftnote.db',
    schemaVersion: 3,
    recoveredBackup: null,
    encrypted: true,
    plaintextBackupCount,
  }
}

function mockSettingsStats() {
  vi.mocked(api.dbStats).mockResolvedValue({
    path: 'C:\\ShiftNote\\shiftnote.db', sizeBytes: 100, taskCount: 1,
    updateCount: 0, schemaVersion: 3, walEnabled: false,
  })
}

it('keeps business views, data loading and creation shortcuts behind the unlock gate', async () => {
  const ui = useUiStore()
  const tasks = useTaskStore()
  const loadTheme = vi.spyOn(ui, 'loadTheme').mockResolvedValue()
  const bootstrap = vi.spyOn(tasks, 'bootstrap').mockResolvedValue()
  const checkUpdates = vi.spyOn(ui, 'autoCheckForUpdates').mockResolvedValue()
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<p>机密事项列表</p>' } },
    ],
  })
  await router.push('/')
  const wrapper = mount(App, {
    global: {
      plugins: [router],
      stubs: {
        AppSidebar: true,
        AppTopbar: true,
        TaskDetail: true,
        TaskEditor: true,
        ConfirmDialog: true,
        ToastHost: true,
      },
    },
  })
  await flushPromises()
  expect(wrapper.text()).toContain('解锁本地数据')
  expect(wrapper.text()).not.toContain('机密事项列表')
  expect(loadTheme).not.toHaveBeenCalled()
  expect(bootstrap).not.toHaveBeenCalled()
  expect(checkUpdates).not.toHaveBeenCalled()
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', ctrlKey: true }))
  expect(ui.editorOpen).toBe(false)

  await wrapper.get('[aria-label="加密口令"]').setValue('existing-password')
  await wrapper.get('form').trigger('submit')
  await flushPromises()
  expect(loadTheme).toHaveBeenCalledOnce()
  expect(bootstrap).toHaveBeenCalledOnce()
  expect(checkUpdates).toHaveBeenCalledOnce()
  expect(wrapper.text()).toContain('机密事项列表')
})

it('requires a confirmed 12-character setup password and explicit legacy migration consent', async () => {
  const wrapper = await unlockScreen(false, true)
  expect(wrapper.text()).toContain('忘记口令将无法恢复数据')
  expect(wrapper.text()).toContain('迁移为口令加密格式')
  await wrapper.get('[aria-label="新口令"]').setValue('short')
  await wrapper.get('[aria-label="再次输入口令"]').setValue('short')
  await wrapper.get('form').trigger('submit')
  expect(wrapper.get('[role="alert"]').text()).toContain('至少需要 12 个字符')
  expect(api.vaultUnlock).not.toHaveBeenCalled()

  const password = ' 一二三四五六七八九十🙂 '
  await wrapper.get('[aria-label="新口令"]').setValue(password)
  await wrapper.get('[aria-label="再次输入口令"]').setValue('different-password')
  await wrapper.get('form').trigger('submit')
  expect(wrapper.get('[role="alert"]').text()).toContain('两次输入的口令不一致')
  expect(api.vaultUnlock).not.toHaveBeenCalled()

  await wrapper.get('[aria-label="新口令"]').setValue(password)
  await wrapper.get('[aria-label="再次输入口令"]').setValue(password)
  await wrapper.get('form').trigger('submit')
  expect(wrapper.get('[role="alert"]').text()).toContain('请先确认加密迁移')
  expect(api.vaultUnlock).not.toHaveBeenCalled()
  await wrapper.get('input[type="checkbox"]').setValue(true)
  await wrapper.get('form').trigger('submit')
  await flushPromises()
  expect(api.vaultUnlock).toHaveBeenCalledWith(password, password, true)
  expect(wrapper.emitted('unlocked')).toHaveLength(1)
})

it('counts Unicode characters rather than UTF-16 units for new passwords', async () => {
  const wrapper = await unlockScreen(false)
  const password = '🙂'.repeat(6)
  await wrapper.get('[aria-label="新口令"]').setValue(password)
  await wrapper.get('[aria-label="再次输入口令"]').setValue(password)
  await wrapper.get('form').trigger('submit')
  expect(wrapper.get('[role="alert"]').text()).toContain('至少需要 12 个字符')
  expect(api.vaultUnlock).not.toHaveBeenCalled()
})

it('clears submitted passwords without storing or logging them and allows failed unlocks to retry', async () => {
  const wrapper = await unlockScreen()
  const persist = vi.spyOn(Storage.prototype, 'setItem')
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
  const log = vi.spyOn(console, 'log').mockImplementation(() => {})
  const error = vi.spyOn(console, 'error').mockImplementation(() => {})
  let reject!: (value: unknown) => void
  vi.mocked(api.vaultUnlock).mockImplementationOnce(
    () => new Promise((_, no) => { reject = no })
  )
  await wrapper.get('[aria-label="加密口令"]').setValue('wrong-secret')
  await wrapper.get('form').trigger('submit')
  expect((wrapper.get('input').element as HTMLInputElement).value).toBe('')
  expect(wrapper.get('button[type="submit"]').attributes('disabled')).toBeDefined()
  expect(JSON.stringify(useVaultStore().$state)).not.toContain('wrong-secret')
  reject(new ApiFailure('口令错误，请重试', 'vault_password'))
  await flushPromises()
  expect(wrapper.get('[role="alert"]').text()).toContain('口令错误')
  expect(useVaultStore().unlocked).toBe(false)

  // Existing vaults remain readable even if a future policy changes setup length.
  await wrapper.get('[aria-label="加密口令"]').setValue(' old ')
  await wrapper.get('form').trigger('submit')
  await flushPromises()
  expect(api.vaultUnlock).toHaveBeenLastCalledWith(' old ', undefined, false)
  expect(useVaultStore().unlocked).toBe(true)
  expect(persist).not.toHaveBeenCalled()
  expect(warn).not.toHaveBeenCalled()
  expect(log).not.toHaveBeenCalled()
  expect(error).not.toHaveBeenCalled()
})

it('does not offer password submission after a status error and supports status retry', async () => {
  vi.mocked(api.vaultStatus).mockRejectedValueOnce(new ApiFailure('读取失败'))
  await useVaultStore().initialize()
  const wrapper = mount(VaultUnlock)
  expect(wrapper.find('input').exists()).toBe(false)
  expect(wrapper.get('[role="alert"]').text()).toBe('读取失败')
  await wrapper.get('button').trigger('click')
  await flushPromises()
  expect(wrapper.find('[aria-label="加密口令"]').exists()).toBe(true)
  expect(useVaultStore().unlocked).toBe(false)
})

it('restores encrypted backups with the original password and clears it on failure and cancel', async () => {
  vi.mocked(api.appInfo).mockResolvedValue(settingsInfo())
  mockSettingsStats()
  vi.mocked(open).mockResolvedValue('C:\\Backups\\other.snbak')
  vi.mocked(api.dbRestore).mockRejectedValueOnce(new ApiFailure('备份口令错误'))
  const wrapper = mount(SettingsView, { attachTo: document.body })
  await flushPromises()
  const restoreButton = wrapper.findAll('button').find((button) => button.text() === '从备份恢复')!
  await restoreButton.trigger('click')
  await flushPromises()
  expect(open).toHaveBeenCalledWith(expect.objectContaining({
    filters: [{ name: 'ShiftNote 备份', extensions: ['snbak', 'db'] }],
  }))
  const input = document.querySelector<HTMLInputElement>('[aria-label="原备份口令"]')!
  input.value = ' original-secret '
  input.dispatchEvent(new Event('input', { bubbles: true }))
  document.querySelector('form[role="dialog"]')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
  await flushPromises()
  expect(api.dbRestore).toHaveBeenCalledWith('C:\\Backups\\other.snbak', ' original-secret ')
  expect(input.value).toBe('')
  expect(document.querySelector('[role="alert"]')!.textContent).toBe('备份口令错误')
  input.value = 'cancelled-secret'
  input.dispatchEvent(new Event('input', { bubbles: true }))
  const cancel = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find((button) => button.textContent === '取消')!
  cancel.click()
  await flushPromises()
  expect(document.querySelector('[role="dialog"]')).toBeNull()
  await restoreButton.trigger('click')
  await flushPromises()
  expect(document.querySelector<HTMLInputElement>('[aria-label="原备份口令"]')!.value).toBe('')
  document.querySelector('form[role="dialog"]')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
  await flushPromises()
  expect(api.dbRestore).toHaveBeenLastCalledWith('C:\\Backups\\other.snbak', undefined)
})

it('shows historical plaintext backups and refreshes their count after encryption', async () => {
  vi.mocked(api.appInfo)
    .mockResolvedValueOnce(settingsInfo(2))
    .mockResolvedValue(settingsInfo(0))
  mockSettingsStats()
  vi.mocked(api.encryptLegacyBackups).mockResolvedValue({ encrypted: 2, remaining: 0 })
  const wrapper = mount(SettingsView)
  await flushPromises()
  expect(wrapper.text()).toContain('口令加密')
  expect(wrapper.text()).toContain('C:\\ShiftNote\\shiftnote.db')
  expect(wrapper.text()).toContain('仍有 2 个旧版明文备份')
  expect(wrapper.text()).toContain('加密成功后原明文文件会移除；失败时保留原文件')
  const encrypt = wrapper.findAll('button').find((button) => button.text() === '加密旧备份')!
  await encrypt.trigger('click')
  await flushPromises()
  expect(api.encryptLegacyBackups).toHaveBeenCalledExactlyOnceWith()
  expect(api.appInfo).toHaveBeenCalledTimes(2)
  expect(wrapper.text()).not.toContain('旧版明文备份')
  expect(useToastStore().items.at(-1)?.message).toContain('已加密 2 个旧备份，原明文文件已移除')
})

it('refreshes counts after a failed legacy-backup migration and keeps retry available', async () => {
  vi.mocked(api.appInfo)
    .mockResolvedValueOnce(settingsInfo(3))
    .mockResolvedValue(settingsInfo(1))
  mockSettingsStats()
  vi.mocked(api.encryptLegacyBackups).mockRejectedValue(new ApiFailure('文件被占用，请重试'))
  const wrapper = mount(SettingsView)
  await flushPromises()
  const encrypt = wrapper.findAll('button').find((button) => button.text() === '加密旧备份')!
  await encrypt.trigger('click')
  await flushPromises()
  expect(api.appInfo).toHaveBeenCalledTimes(2)
  expect(wrapper.text()).toContain('仍有 1 个旧版明文备份')
  expect(encrypt.attributes('disabled')).toBeUndefined()
  expect(useToastStore().items.at(-1)?.message).toBe('文件被占用，请重试')
})

it('treats missing historical-backup fields in old browser fixtures as zero', async () => {
  const oldInfo = settingsInfo()
  delete oldInfo.encrypted
  delete oldInfo.plaintextBackupCount
  vi.mocked(api.appInfo).mockResolvedValue(oldInfo)
  mockSettingsStats()
  const wrapper = mount(SettingsView)
  await flushPromises()
  expect(wrapper.text()).not.toContain('旧版明文备份')
  expect(wrapper.findAll('span').some((span) => span.text() === '口令加密')).toBe(false)
  expect(wrapper.findAll('button').some((button) => button.text() === '立即备份')).toBe(true)
  expect(wrapper.findAll('button').some((button) => button.text() === '从备份恢复')).toBe(true)
})

it('allows first-run setup to be skipped and clears unused password inputs', async () => {
  const wrapper = await unlockScreen(false)
  await wrapper.get('[aria-label="新口令"]').setValue('discard-this-password')
  const skip = wrapper.findAll('button').find(button => button.text() === '暂不加密，直接进入')!
  await skip.trigger('click')
  await flushPromises()
  expect(api.vaultSkip).toHaveBeenCalledExactlyOnceWith()
  expect(api.vaultUnlock).not.toHaveBeenCalled()
  expect(useVaultStore().unlocked).toBe(true)
  expect(useVaultStore().configured).toBe(false)
  expect(wrapper.get('[aria-label="新口令"]').element).toHaveProperty('value', '')
  expect(wrapper.emitted('unlocked')).toHaveLength(1)
})

it('keeps encrypted data behind its password gate and preserves skip errors for retry', async () => {
  const encrypted = await unlockScreen(true)
  expect(encrypted.text()).not.toContain('暂不加密，直接进入')
  expect(await useVaultStore().skip()).toBe(false)
  expect(api.vaultSkip).not.toHaveBeenCalled()
  encrypted.unmount()
  const plain = await unlockScreen(false)
  vi.mocked(api.vaultSkip).mockRejectedValueOnce(new ApiFailure('数据被占用'))
  await plain.findAll('button').find(button => button.text() === '暂不加密，直接进入')!.trigger('click')
  await flushPromises()
  expect(plain.get('[role="alert"]').text()).toBe('数据被占用')
  expect(useVaultStore().unlocked).toBe(false)
})

it('opens an already-skipped store without requesting a password', async () => {
  vi.mocked(api.vaultStatus).mockResolvedValue({ configured: false, unlocked: true, legacy: false })
  await useVaultStore().initialize()
  expect(useVaultStore().unlocked).toBe(true)
  expect(useVaultStore().configured).toBe(false)
  expect(api.vaultUnlock).not.toHaveBeenCalled()
  expect(api.vaultSkip).not.toHaveBeenCalled()
})

it('enables encryption from settings, clearing secrets on failure and retry', async () => {
  useVaultStore().unlocked = true
  vi.mocked(api.appInfo).mockResolvedValue({ ...settingsInfo(), encrypted: false })
  mockSettingsStats()
  const wrapper = mount(SettingsView)
  await flushPromises()
  expect(wrapper.text()).toContain('数据库和备份以明文保存')
  await wrapper.findAll('button').find(button => button.text() === '启用口令加密')!.trigger('click')
  await wrapper.get('[aria-label="设置新口令"]').setValue('settings-secret-password')
  await wrapper.get('[aria-label="确认设置口令"]').setValue('settings-secret-password')
  vi.mocked(api.vaultEnableEncryption).mockRejectedValueOnce(new ApiFailure('文件被占用'))
  await wrapper.get('form').trigger('submit')
  await flushPromises()
  expect(wrapper.get('[role="alert"]').text()).toBe('文件被占用')
  expect(wrapper.get('[aria-label="设置新口令"]').element).toHaveProperty('value', '')
  expect(useVaultStore().configured).toBe(false)
  await wrapper.get('[aria-label="设置新口令"]').setValue('settings-secret-password')
  await wrapper.get('[aria-label="确认设置口令"]').setValue('settings-secret-password')
  vi.mocked(api.appInfo).mockResolvedValue(settingsInfo())
  await wrapper.get('form').trigger('submit')
  await flushPromises()
  expect(api.vaultEnableEncryption).toHaveBeenLastCalledWith('settings-secret-password', 'settings-secret-password')
  expect(useVaultStore().configured).toBe(true)
  expect(wrapper.find('[aria-label="设置新口令"]').exists()).toBe(false)
  expect(wrapper.text()).toContain('口令加密')
})
