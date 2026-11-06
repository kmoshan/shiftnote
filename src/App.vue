<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'

import AppSidebar from '@/components/AppSidebar.vue'
import AppTopbar from '@/components/AppTopbar.vue'
import TaskDetail from '@/components/TaskDetail.vue'
import TaskEditor from '@/components/TaskEditor.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import ToastHost from '@/components/ToastHost.vue'
import UpdateBanner from '@/components/UpdateBanner.vue'
import { isTauriRuntime } from '@/services/api'
import { useTaskStore } from '@/stores/tasks'
import { useUiStore } from '@/stores/ui'

const tasks = useTaskStore()
const ui = useUiStore()

function handleKeydown(event: KeyboardEvent) {
  const withModifier = event.ctrlKey || event.metaKey
  const key = event.key.toLowerCase()

  if (withModifier && key === 'n') {
    event.preventDefault()
    ui.openCreate()
    return
  }
  if (withModifier && key === 'f') {
    event.preventDefault()
    window.dispatchEvent(new CustomEvent('shiftnote:focus-search'))
    return
  }
  if (event.key === 'Escape' && ui.editorOpen) {
    ui.closeEditor()
  }
}

onMounted(async () => {
  window.addEventListener('keydown', handleKeydown)
  if (!isTauriRuntime()) return
  await ui.loadTheme()
  await tasks.bootstrap()
  // 静默检查更新：12 小时内只查一次，失败不打扰
  void ui.autoCheckForUpdates()
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown)
})
</script>

<template>
  <div v-if="!isTauriRuntime()" class="flex h-full items-center justify-center bg-canvas p-8">
    <div class="max-w-md rounded-xl border border-line bg-panel p-6 text-center shadow-sm">
      <h1 class="mb-2 text-base font-semibold text-ink">请在桌面环境中打开 ShiftNote</h1>
      <p class="mb-3 text-[12.5px] leading-relaxed text-ink-2">
        当前页面运行在普通浏览器中，无法访问本地 SQLite 数据库。请使用
        <code class="rounded bg-panel-2 px-1.5 py-0.5 font-mono text-[11.5px]">npm run app:dev</code>
        启动桌面窗口。
      </p>
      <p class="text-[11.5px] text-ink-3">ShiftNote 不会上传任何数据，全部记录保存在本机。</p>
    </div>
  </div>

  <div v-else class="flex h-full w-full overflow-hidden bg-canvas text-ink">
    <AppSidebar />

    <div class="flex min-w-0 flex-1 flex-col">
      <AppTopbar />
      <UpdateBanner />
      <div class="flex min-h-0 flex-1">
        <router-view class="min-w-0 flex-1" />
        <TaskDetail v-if="tasks.selectedId" />
      </div>
    </div>

    <TaskEditor v-if="ui.editorOpen" />
    <ConfirmDialog />
    <ToastHost />
  </div>
</template>
