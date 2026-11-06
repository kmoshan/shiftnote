import { defineStore } from 'pinia'
import { ref } from 'vue'

import { api, toApiFailure } from '@/services/api'
import type { CreateTaskInput, Task } from '@/types'

export type ThemeMode = 'light' | 'dark'

export const useUiStore = defineStore('ui', () => {
  const theme = ref<ThemeMode>('light')

  const editorOpen = ref(false)
  const editingTask = ref<Task | null>(null)
  const editorSeedTitle = ref('')
  const editorSeedPriority = ref<number | null>(null)

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
  }
})
