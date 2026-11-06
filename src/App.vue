<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { useRoute } from 'vue-router'

import AppSidebar from '@/components/AppSidebar.vue'
import AppTopbar from '@/components/AppTopbar.vue'
import TaskDetail from '@/components/TaskDetail.vue'
import TaskEditor from '@/components/TaskEditor.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import ToastHost from '@/components/ToastHost.vue'
import VaultUnlock from '@/components/VaultUnlock.vue'
import UpdateBanner from '@/components/UpdateBanner.vue'
import { isTauriRuntime } from '@/services/api'
import { useTaskStore } from '@/stores/tasks'
import { useUiStore } from '@/stores/ui'
import { useVaultStore } from '@/stores/vault'

const tasks = useTaskStore()
const ui = useUiStore()
const vault = useVaultStore()
const route = useRoute()

function handleKeydown(event: KeyboardEvent) {
  if (!vault.unlocked) return
  if (tasks.batchBusy && (event.ctrlKey || event.metaKey)) {
    event.preventDefault()
    return
  }
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

async function loadWorkspace() {
  await ui.loadTheme()
  await tasks.bootstrap()
  void ui.autoCheckForUpdates()
}

watch(
  () => vault.unlocked,
  (unlocked) => {
    if (unlocked && isTauriRuntime()) void loadWorkspace()
  },
  { immediate: true }
)

onMounted(async () => {
  window.addEventListener('keydown', handleKeydown)
  if (!isTauriRuntime()) return
  await vault.initialize()
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeydown)
})
</script>

<template>
  <div
    v-if="!isTauriRuntime()"
    class="flex h-full items-center justify-center bg-canvas p-8"
  >
    <div
      class="max-w-md rounded-xl border border-line bg-panel p-6 text-center shadow-sm"
    >
      <h1 class="mb-2 text-base font-semibold text-ink">
        请在桌面环境中打开 ShiftNote
      </h1>
      <p class="mb-3 text-[12.5px] leading-relaxed text-ink-2">
        当前页面运行在普通浏览器中，无法访问本地 SQLite 数据库。请使用
        <code class="rounded bg-panel-2 px-1.5 py-0.5 font-mono text-[11.5px]"
          >npm run app:dev</code
        >
        启动桌面窗口。
      </p>
      <p class="text-[11.5px] text-ink-3">
        事项与交接记录保存在本机，业务功能无需服务器。
      </p>
    </div>
  </div>

  <VaultUnlock v-else-if="!vault.unlocked" />

  <div v-else class="flex h-full w-full overflow-hidden bg-canvas text-ink">
    <AppSidebar />

    <div class="flex min-w-0 flex-1 flex-col">
      <AppTopbar />
      <UpdateBanner />
      <div class="flex min-h-0 flex-1">
        <router-view class="min-w-0 flex-1" />
        <TaskDetail v-if="tasks.selectedId && route.meta.view" />
      </div>
    </div>

    <TaskEditor v-if="ui.editorOpen" />
    <ConfirmDialog />
    <ToastHost />
  </div>
</template>
