import { defineStore } from 'pinia'
import { ref } from 'vue'
import { openUrl } from '@tauri-apps/plugin-opener'

import { api, toApiFailure } from '@/services/api'
import { useToastStore } from '@/stores/toast'
import type { CreateTaskInput, Task, UpdateInfo } from '@/types'

export type ThemeMode = 'light' | 'dark'

/** 自动检查更新的最小间隔：12 小时 */
const AUTO_CHECK_INTERVAL = 12 * 60 * 60 * 1000
const LAST_CHECK_KEY = 'update.lastCheckAt'

export const useUiStore = defineStore('ui', () => {
  const toast = useToastStore()

  const theme = ref<ThemeMode>('light')

  const editorOpen = ref(false)
  const editingTask = ref<Task | null>(null)
  const editorSeedTitle = ref('')
  const editorSeedPriority = ref<number | null>(null)

  // 更新检查状态
  const updateInfo = ref<UpdateInfo | null>(null)
  const updateChecking = ref(false)
  const updateDismissed = ref(false)
  const updateError = ref<string | null>(null)

  function applyTheme(value: ThemeMode) {
    theme.value = value
    const root = document.documentElement
    root.classList.toggle('dark', value === 'dark')
    root.style.colorScheme = value
  }

  async function setTheme(value: ThemeMode) {
    applyTheme(value)
    try {
      await api.settingsSet('theme', value)
    } catch (error) {
      console.warn('[shiftnote] 主题设置保存失败', toApiFailure(error).message)
    }
  }

  async function toggleTheme() {
    await setTheme(theme.value === 'dark' ? 'light' : 'dark')
  }

  async function loadTheme() {
    try {
      const saved = await api.settingsGet('theme')
      // 规范要求默认浅色；只有在用户显式选择过深色时才使用深色
      applyTheme(saved === 'dark' ? 'dark' : 'light')
    } catch {
      applyTheme('light')
    }
  }

  function openCreate(seed?: Partial<CreateTaskInput>) {
    editingTask.value = null
    editorSeedTitle.value = seed?.title ?? ''
    editorSeedPriority.value = seed?.priority ?? null
    editorOpen.value = true
  }

  function openEdit(task: Task) {
    editingTask.value = task
    editorSeedTitle.value = ''
    editorSeedPriority.value = null
    editorOpen.value = true
  }

  function closeEditor() {
    editorOpen.value = false
    editingTask.value = null
    editorSeedTitle.value = ''
    editorSeedPriority.value = null
  }

  /**
   * 检查更新。manual=true 时给出明确反馈，否则静默（失败只记录不打扰）。
   */
  async function checkForUpdates(options: { manual?: boolean } = {}) {
    if (updateChecking.value) return

    updateChecking.value = true
    updateError.value = null
    try {
      const info = await api.checkUpdate()
      updateInfo.value = info
      updateDismissed.value = false
      void api
        .settingsSet(LAST_CHECK_KEY, String(info.checkedAt))
        .catch(() => undefined)

      if (options.manual) {
        if (info.hasUpdate) {
          toast.success(`发现新版本 v${info.latestVersion}`)
        } else {
          toast.success(`当前版本 v${info.currentVersion} 无需更新`)
        }
      }
    } catch (error) {
      const message = toApiFailure(error).message
      updateError.value = message
      if (options.manual) toast.error(message)
    } finally {
      updateChecking.value = false
    }
  }

  /** 启动时的静默检查：12 小时内只查一次，失败不打扰用户 */
  async function autoCheckForUpdates() {
    try {
      const saved = await api.settingsGet(LAST_CHECK_KEY)
      const last = Number(saved ?? 0)
      const now = Date.now()
      if (Number.isFinite(last) && last > 0 && last <= now && now - last < AUTO_CHECK_INTERVAL) return
    } catch {
      // 读不到配置也继续检查
    }
    await checkForUpdates()
  }

  function dismissUpdate() {
    updateDismissed.value = true
  }

  async function openReleasePage(url?: string | null) {
    const target = url ?? updateInfo.value?.releaseUrl
    if (!target) return
    if (!/^https:\/\/github\.com\/kmoshan\/shiftnote\/releases(?:\/tag\/[^/?#]+)?$/.test(target)) {
      toast.error('更新地址无效，仅允许打开项目的 GitHub Releases 页面')
      return
    }
    try {
      await openUrl(target)
    } catch (error) {
      toast.error(`无法打开浏览器：${toApiFailure(error).message}`)
    }
  }

  return {
    theme,
    applyTheme,
    setTheme,
    toggleTheme,
    loadTheme,
    editorOpen,
    editingTask,
    editorSeedTitle,
    editorSeedPriority,
    openCreate,
    openEdit,
    closeEditor,
    updateInfo,
    updateChecking,
    updateDismissed,
    updateError,
    checkForUpdates,
    autoCheckForUpdates,
    dismissUpdate,
    openReleasePage,
  }
})
