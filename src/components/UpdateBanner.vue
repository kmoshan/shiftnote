<script setup lang="ts">
import { Download, X } from '@lucide/vue'

import { useUiStore } from '@/stores/ui'

const ui = useUiStore()
</script>

<template>
  <div
    v-if="ui.updateInfo?.hasUpdate && !ui.updateDismissed"
    class="flex shrink-0 items-center gap-2 border-b border-line bg-accent-soft px-4 py-1.5"
  >
    <Download :size="13" class="shrink-0 text-accent" />
    <p class="min-w-0 flex-1 truncate text-[11.5px] text-ink-2">
      发现新版本
      <span class="font-semibold text-accent">v{{ ui.updateInfo.latestVersion }}</span>
      <span class="text-ink-3">（当前 v{{ ui.updateInfo.currentVersion }}）</span>
    </p>
    <button
      type="button"
      class="shrink-0 rounded px-1.5 py-0.5 text-[11.5px] font-medium text-accent hover:bg-panel/70"
      @click="ui.openReleasePage()"
    >
      查看更新
    </button>
    <button
      type="button"
      class="grid h-6 w-6 shrink-0 place-items-center rounded text-ink-3 hover:bg-panel/70 hover:text-ink"
      title="本次不再提示"
      @click="ui.dismissUpdate()"
    >
      <X :size="13" />
    </button>
  </div>
</template>
