<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import {
  Check,
  Copy,
  Database,
  Download,
  Moon,
  RefreshCw,
  RotateCcw,
  Save,
  Sun,
} from '@lucide/vue'

import { api, toApiFailure } from '@/services/api'
import { formatBytes, formatSmart } from '@/services/format'
import { useConfirmStore } from '@/stores/confirm'
import { useTaskStore } from '@/stores/tasks'
import { useToastStore } from '@/stores/toast'
import { useUiStore } from '@/stores/ui'
import type { AppInfo, DbStats } from '@/types'

const ui = useUiStore()
const toast = useToastStore()
const confirm = useConfirmStore()
const tasks = useTaskStore()

const info = ref<AppInfo | null>(null)
const stats = ref<DbStats | null>(null)
const loading = ref(true)
const backingUp = ref(false)
const restoring = ref(false)
const lastBackupPath = ref<string | null>(null)
const lastRestoreSnapshot = ref<string | null>(null)

const backupDir = computed(() => (info.value ? `${info.value.dataDir}\\backups` : '—'))

const updateStatus = computed(() => {
  if (ui.updateChecking) return '正在连接 GitHub 检查…'
  if (ui.updateError) return ui.updateError
  const latest = ui.updateInfo
  if (!latest) return '尚未检查更新'
  const when = formatSmart(latest.checkedAt)
  if (latest.hasUpdate) {
    return `发现新版本 v${latest.latestVersion}（${when} 检查，当前 v${latest.currentVersion}）`
  }
  const proxyHint = latest.usedProxy ? '，经系统代理' : ''
  return `已是最新版本（${when} 检查${proxyHint}）`
})

async function load() {
  loading.value = true
  try {
    const [appInfo, dbStats] = await Promise.all([api.appInfo(), api.dbStats()])
    info.value = appInfo
    stats.value = dbStats
    if (appInfo.recoveredBackup) {
      toast.error(`数据库曾损坏，原文件已备份到：${appInfo.recoveredBackup}`)
    }
  } catch (caught) {
    toast.error(toApiFailure(caught).message)
  } finally {
    loading.value = false
  }
}

async function backupNow() {
  if (backingUp.value) return
  backingUp.value = true
  try {
    const result = await api.dbBackup()
    lastBackupPath.value = result.path
    toast.success(`备份完成：${formatBytes(result.sizeBytes)}`)
  } catch (caught) {
    toast.error(toApiFailure(caught).message)
  } finally {
    backingUp.value = false
  }
}

async function restoreFromBackup() {
  if (restoring.value) return

  let selected: string | null = null
  try {
    const picked = await open({
      multiple: false,
      directory: false,
      title: '选择 ShiftNote 备份文件',
      filters: [{ name: 'ShiftNote 备份', extensions: ['db'] }],
    })
    selected = typeof picked === 'string' ? picked : null
  } catch (caught) {
    toast.error(toApiFailure(caught).message)
    return
  }
  if (!selected) return

  const ok = await confirm.ask({
    title: '从备份恢复数据？',
    message:
      '当前数据库会被备份文件覆盖。覆盖前会自动把当前数据快照到备份目录（pre-restore-*.db），必要时可用它回退。',
    confirmText: '开始恢复',
    danger: true,
  })
  if (!ok) return

  restoring.value = true
  try {
    const result = await api.dbRestore(selected)
    lastRestoreSnapshot.value = result.snapshotPath

    // 恢复后重新加载列表、主题与统计信息
    tasks.selectTask(null)
    await tasks.bootstrap()
    await ui.loadTheme()
    await load()

    toast.success(
      `已恢复 ${result.taskCount} 条事项、${result.updateCount} 条进展（结构版本 v${result.schemaVersion}）`
    )
  } catch (caught) {
    toast.error(toApiFailure(caught).message)
  } finally {
    restoring.value = false
  }
}

async function copyPath(path: string | undefined) {
  if (!path) return
  try {
    await navigator.clipboard.writeText(path)
    toast.success('路径已复制到剪贴板')
  } catch {
    toast.error('复制失败，请手动选择文本')
  }
}

onMounted(load)
</script>

<template>
  <section class="min-h-0 flex-1 overflow-y-auto">
    <div class="mx-auto w-full max-w-[720px] space-y-5 px-6 py-5">
      <header>
        <h1 class="text-[15px] font-semibold text-ink">设置</h1>
        <p class="mt-0.5 text-[11.5px] text-ink-3">
          所有配置与数据都保存在本机，不依赖账号或服务器。
        </p>
      </header>

      <!-- 外观 -->
      <div class="rounded-lg border border-line bg-panel">
        <div class="border-b border-line px-3.5 py-2.5">
          <h2 class="text-[12.5px] font-semibold text-ink">外观</h2>
          <p class="text-[11px] text-ink-3">浅色为默认主题，深色适合夜间值守。</p>
        </div>
        <div class="grid grid-cols-2 gap-2.5 p-3.5">
          <button
            type="button"
            class="flex items-center gap-2.5 rounded-md border px-3 py-2.5 text-left transition-colors"
            :class="ui.theme === 'light' ? 'border-accent bg-accent-soft' : 'border-line hover:border-line-strong'"
            @click="ui.setTheme('light')"
          >
            <Sun :size="16" class="text-accent" />
            <span class="flex-1">
              <span class="block text-[12.5px] font-medium text-ink">浅色模式</span>
              <span class="block text-[11px] text-ink-3">默认，日间使用</span>
            </span>
            <Check v-if="ui.theme === 'light'" :size="14" class="text-accent" />
          </button>

          <button
            type="button"
            class="flex items-center gap-2.5 rounded-md border px-3 py-2.5 text-left transition-colors"
            :class="ui.theme === 'dark' ? 'border-accent bg-accent-soft' : 'border-line hover:border-line-strong'"
            @click="ui.setTheme('dark')"
          >
            <Moon :size="16" class="text-accent" />
            <span class="flex-1">
              <span class="block text-[12.5px] font-medium text-ink">深色模式</span>
              <span class="block text-[11px] text-ink-3">夜间值守更舒适</span>
            </span>
            <Check v-if="ui.theme === 'dark'" :size="14" class="text-accent" />
          </button>
        </div>
      </div>

      <!-- 数据 -->
      <div class="rounded-lg border border-line bg-panel">
        <div class="flex items-center gap-2 border-b border-line px-3.5 py-2.5">
          <Database :size="14" class="text-ink-3" />
          <h2 class="text-[12.5px] font-semibold text-ink">本地数据</h2>
          <span v-if="stats" class="ml-auto text-[11px] tabular-nums text-ink-3">
            {{ formatBytes(stats.sizeBytes) }}
          </span>
        </div>

        <div v-if="loading" class="px-3.5 py-4 text-[11.5px] text-ink-3">正在读取数据库信息…</div>

        <div v-else-if="stats" class="divide-y divide-[color:var(--sn-line)]">
          <div class="px-3.5 py-2.5">
            <div class="flex items-center gap-2">
              <span class="text-[11.5px] text-ink-3">数据库文件</span>
              <button
                type="button"
                class="ml-auto flex items-center gap-1 text-[11px] text-accent hover:underline"
                @click="copyPath(stats.path)"
              >
                <Copy :size="11" />
                复制路径
              </button>
            </div>
            <p class="mt-0.5 break-all font-mono text-[11.5px] text-ink-2">{{ stats.path }}</p>
          </div>

          <div class="grid grid-cols-2 gap-3 px-3.5 py-2.5 sm:grid-cols-4">
            <div>
              <div class="text-[10.5px] uppercase tracking-wide text-ink-3">事项总数</div>
              <div class="text-[13px] font-semibold tabular-nums text-ink">{{ stats.taskCount }}</div>
            </div>
            <div>
              <div class="text-[10.5px] uppercase tracking-wide text-ink-3">进展记录</div>
              <div class="text-[13px] font-semibold tabular-nums text-ink">{{ stats.updateCount }}</div>
            </div>
            <div>
              <div class="text-[10.5px] uppercase tracking-wide text-ink-3">结构版本</div>
              <div class="text-[13px] font-semibold tabular-nums text-ink">v{{ stats.schemaVersion }}</div>
            </div>
            <div>
              <div class="text-[10.5px] uppercase tracking-wide text-ink-3">日志模式</div>
              <div class="text-[13px] font-semibold text-ink">{{ stats.walEnabled ? 'WAL' : '缺省' }}</div>
            </div>
          </div>

          <div class="flex flex-wrap items-center gap-2 px-3.5 py-2.5">
            <button type="button" class="btn h-8" :disabled="backingUp" @click="backupNow">
              <Save :size="13" />
              {{ backingUp ? '备份中…' : '立即备份' }}
            </button>
            <button type="button" class="btn h-8" :disabled="restoring" @click="restoreFromBackup">
              <RotateCcw :size="13" />
              {{ restoring ? '恢复中…' : '从备份恢复' }}
            </button>
            <span class="text-[11px] text-ink-3">
              备份目录：<span class="font-mono">{{ backupDir }}</span>
            </span>
            <p v-if="lastBackupPath" class="w-full break-all font-mono text-[10.5px] text-success">
              最近一次备份：{{ lastBackupPath }}
            </p>
            <p v-if="lastRestoreSnapshot" class="w-full break-all font-mono text-[10.5px] text-ink-3">
              恢复前的数据已快照到：{{ lastRestoreSnapshot }}
            </p>
            <p class="w-full text-[10.5px] leading-relaxed text-ink-3">
              恢复会覆盖当前数据；系统会在覆盖前自动快照当前数据库，恢复失败时自动回滚。
            </p>
          </div>
        </div>
      </div>

      <!-- 关于 -->
      <div class="rounded-lg border border-line bg-panel">
        <div class="flex items-center gap-2 border-b border-line px-3.5 py-2.5">
          <h2 class="text-[12.5px] font-semibold text-ink">关于 ShiftNote</h2>
          <button
            type="button"
            class="ml-auto btn h-7"
            :disabled="ui.updateChecking"
            title="从 GitHub Releases 检查是否有新版本"
            @click="ui.checkForUpdates({ manual: true })"
          >
            <RefreshCw :size="12" :class="ui.updateChecking ? 'animate-spin' : ''" />
            {{ ui.updateChecking ? '检查中…' : '检测更新' }}
          </button>
        </div>

        <div class="space-y-1.5 px-3.5 py-3 text-[11.5px] text-ink-2">
          <p>版本 {{ info?.version ?? '—' }} · Tauri 2 + Vue 3 + SQLite</p>

          <div class="flex flex-wrap items-center gap-2">
            <p
              class="min-w-0 flex-1"
              :class="[
                ui.updateError ? 'text-p0' : '',
                ui.updateInfo?.hasUpdate && !ui.updateError ? 'text-accent' : 'text-ink-3',
              ]"
            >
              {{ updateStatus }}
            </p>
            <button
              v-if="ui.updateInfo?.hasUpdate"
              type="button"
              class="btn btn-primary h-7 shrink-0"
              @click="ui.openReleasePage()"
            >
              <Download :size="12" />
              下载 v{{ ui.updateInfo.latestVersion }}
            </button>
          </div>

          <pre
            v-if="ui.updateInfo?.hasUpdate && ui.updateInfo.notes"
            class="max-h-[132px] overflow-y-auto whitespace-pre-wrap break-words rounded-md bg-panel-2 p-2 text-[10.5px] leading-relaxed text-ink-2"
          >{{ ui.updateInfo.notes }}</pre>

          <p class="text-ink-3">
            轻量级交接班与事项跟踪工具。数据完全保存在本地，默认不上传任何内容。
          </p>
          <p class="text-ink-3">
            检查更新只会向 GitHub 读取公开的版本号，不会发送任何本地数据。
          </p>
          <p v-if="info" class="text-ink-3">数据目录：{{ info.dataDir }}</p>
          <p v-if="info?.recoveredBackup" class="text-p0">
            上次启动检测到数据库损坏，已自动备份并重建：{{ info.recoveredBackup }}
          </p>
        </div>
      </div>
    </div>
  </section>
</template>
