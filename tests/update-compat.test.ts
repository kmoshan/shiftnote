import { beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { openUrl } from '@tauri-apps/plugin-opener'
import { api, ApiFailure } from '@/services/api'
import { useUiStore } from '@/stores/ui'
import { useToastStore } from '@/stores/toast'
import type { UpdateInfo } from '@/types'

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }))
vi.mock('@/services/api', async (load) => {
  const actual = await load<typeof import('@/services/api')>()
  return { ...actual, api: { ...actual.api, checkUpdate: vi.fn(), settingsGet: vi.fn(), settingsSet: vi.fn() } }
})

function update(overrides: Partial<UpdateInfo> = {}): UpdateInfo {
  return {
    currentVersion: '0.2.1', latestVersion: '0.2.2', hasUpdate: true,
    releaseName: '更新', releaseUrl: 'https://github.com/kmoshan/shiftnote/releases/tag/v0.2.2',
    publishedAt: null, notes: '中文更新说明', checkedAt: Date.now(), usedProxy: false,
    ...overrides,
  }
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.mocked(api.settingsGet).mockResolvedValue(null)
  vi.mocked(api.settingsSet).mockResolvedValue(undefined)
  vi.mocked(api.checkUpdate).mockResolvedValue(update())
})

it('uses encrypted settings for the 12-hour cache and ignores future timestamps', async () => {
  const ui = useUiStore()
  vi.mocked(api.settingsGet).mockResolvedValueOnce(String(Date.now() - 60_000))
  await ui.autoCheckForUpdates()
  expect(api.checkUpdate).not.toHaveBeenCalled()
  vi.mocked(api.settingsGet).mockResolvedValueOnce(String(Date.now() + 24 * 60 * 60 * 1000))
  await ui.autoCheckForUpdates()
  expect(api.checkUpdate).toHaveBeenCalledOnce()
  expect(api.settingsSet).toHaveBeenCalledWith('update.lastCheckAt', expect.any(String))
})

it('checks automatically when a restored backup has no update timestamp', async () => {
  await useUiStore().autoCheckForUpdates()
  expect(api.checkUpdate).toHaveBeenCalledOnce()
  expect(useUiStore().updateInfo?.hasUpdate).toBe(true)
})

it('deduplicates overlapping manual and automatic requests', async () => {
  let resolve!: (info: UpdateInfo) => void
  vi.mocked(api.checkUpdate).mockImplementationOnce(() => new Promise(done => { resolve = done }))
  const ui = useUiStore()
  const first = ui.checkForUpdates()
  await ui.checkForUpdates({ manual: true })
  expect(api.checkUpdate).toHaveBeenCalledOnce()
  expect(ui.updateChecking).toBe(true)
  resolve(update())
  await first
  expect(ui.updateChecking).toBe(false)
})

it('reports no update for a local build newer than the public release without offering downgrade', async () => {
  vi.mocked(api.checkUpdate).mockResolvedValue(update({ latestVersion: '0.2.0', hasUpdate: false }))
  await useUiStore().checkForUpdates({ manual: true })
  expect(useUiStore().updateInfo?.hasUpdate).toBe(false)
  expect(useToastStore().items.at(-1)?.message).toBe('当前版本 v0.2.1 无需更新')
})

it('allows manual retry after network failure and keeps startup failure silent', async () => {
  vi.mocked(api.checkUpdate).mockRejectedValueOnce(new ApiFailure('连接失败'))
  const ui = useUiStore()
  await ui.autoCheckForUpdates()
  expect(ui.updateError).toBe('连接失败')
  expect(useToastStore().items).toHaveLength(0)
  await ui.checkForUpdates({ manual: true })
  expect(ui.updateError).toBeNull()
  expect(useToastStore().items.at(-1)?.message).toContain('发现新版本')
})

it('opens only this projects release page and never downloads or installs files', async () => {
  const ui = useUiStore()
  for (const url of ['file:///C:/notes.txt', 'https://example.com/', 'https://github.com/other/repo/releases', 'https://user@github.com/kmoshan/shiftnote/releases']) {
    await ui.openReleasePage(url)
  }
  expect(openUrl).not.toHaveBeenCalled()
  await ui.openReleasePage('https://github.com/kmoshan/shiftnote/releases/tag/v0.2.2')
  expect(openUrl).toHaveBeenCalledExactlyOnceWith('https://github.com/kmoshan/shiftnote/releases/tag/v0.2.2')
})
