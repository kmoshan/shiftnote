<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { Check, Copy, Database, Moon, RotateCcw, Save, Sun } from '@lucide/vue'

import { api, toApiFailure } from '@/services/api'
import { formatBytes } from '@/services/format'
import { useTaskStore } from '@/stores/tasks'
import { useHandoverStore } from '@/stores/handovers'
import { useToastStore } from '@/stores/toast'
import { useUiStore } from '@/stores/ui'
import { useVaultStore } from '@/stores/vault'
import type { AppInfo, DbStats } from '@/types'

const ui = useUiStore()
const toast = useToastStore()
const tasks = useTaskStore()
const handovers = useHandoverStore()
const vault = useVaultStore()

const info = ref<AppInfo | null>(null)
const stats = ref<DbStats | null>(null)
const loading = ref(true)
const backingUp = ref(false)
const restoring = ref(false)
const encryptingLegacyBackups = ref(false)
const lastBackupPath = ref<string | null>(null)
const lastRestoreSnapshot = ref<string | null>(null)
const restoreDialogOpen = ref(false)
const selectedBackupPath = ref<string | null>(null)
const restorePassword = ref('')
const restoreError = ref('')
const encryptionFormOpen = ref(false)
const newPassword = ref('')
const confirmPassword = ref('')
const encryptionError = ref('')

const backupDir = computed(() =>
  info.value ? `${info.value.dataDir}\\backups` : '—'
)
const plaintextBackupCount = computed(() => info.value?.plaintextBackupCount ?? 0)

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
  if (backingUp.value || encryptingLegacyBackups.value || vault.busy) return
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

async function encryptLegacyBackups() {
  if (encryptingLegacyBackups.value || backingUp.value || restoring.value) return
  encryptingLegacyBackups.value = true
  try {
    const result = await api.encryptLegacyBackups()
    if (result.remaining > 0) {
      toast.info(`已加密 ${result.encrypted} 个旧备份，仍有 ${result.remaining} 个明文备份，请重试`)
    } else {
      toast.success(`已加密 ${result.encrypted} 个旧备份，原明文文件已移除`)
    }
  } catch (caught) {
    toast.error(toApiFailure(caught).message)
  } finally {
    try {
      info.value = await api.appInfo()
    } catch (caught) {
      toast.error(toApiFailure(caught).message)
    }
    encryptingLegacyBackups.value = false
  }
}

async function restoreFromBackup() {
  if (restoring.value || encryptingLegacyBackups.value || vault.busy) return

  let selected: string | null = null
  try {
    const picked = await open({
      multiple: false,
      directory: false,
      title: '选择 ShiftNote 备份文件',
      filters: [{ name: 'ShiftNote 备份', extensions: ['snbak', 'db'] }],
    })
    selected = typeof picked === 'string' ? picked : null
  } catch (caught) {
    toast.error(toApiFailure(caught).message)
    return
  }
  if (!selected) return

  selectedBackupPath.value = selected
  restorePassword.value = ''
  restoreError.value = ''
  restoreDialogOpen.value = true
}

function cancelRestore() {
  if (restoring.value) return
  restorePassword.value = ''
  selectedBackupPath.value = null
  restoreDialogOpen.value = false
  restoreError.value = ''
}

async function restoreSelectedBackup() {
  if (restoring.value || !selectedBackupPath.value) return
  const selected = selectedBackupPath.value
  const submittedPassword = restorePassword.value || undefined
  restorePassword.value = ''
  restoreError.value = ''

  restoring.value = true
  try {
    const result = await api.dbRestore(selected, submittedPassword)
    lastRestoreSnapshot.value = result.snapshotPath
    restoreDialogOpen.value = false
    selectedBackupPath.value = null

    // 恢复后重新加载列表、主题与统计信息
    tasks.resetAfterRestore()
    handovers.resetAfterRestore()
    await tasks.bootstrap()
    await ui.loadTheme()
    await load()

    toast.success(
      `已恢复 ${result.taskCount} 条事项、${result.updateCount} 条进展（结构版本 v${result.schemaVersion}）`
    )
  } catch (caught) {
    restoreError.value = toApiFailure(caught).message
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

function cancelEncryption() {
  if (vault.busy) return
  newPassword.value = ''
  confirmPassword.value = ''
  encryptionError.value = ''
  encryptionFormOpen.value = false
}

async function enableEncryption() {
  if (vault.busy || backingUp.value || restoring.value || restoreDialogOpen.value) return
  encryptionError.value = ''
  const password = newPassword.value
  const confirmation = confirmPassword.value
  newPassword.value = ''
  confirmPassword.value = ''
  if (Array.from(password).length < 12) {
    encryptionError.value = '新口令至少需要 12 个字符'
    return
  }
  if (password !== confirmation) {
    encryptionError.value = '两次输入的口令不一致，请重新输入'
    return
  }
  if (await vault.enableEncryption(password, confirmation)) {
    encryptionFormOpen.value = false
    if (info.value) info.value.encrypted = true
    await load()
    toast.success('已启用口令加密，下次启动需要输入口令')
  } else {
    encryptionError.value = vault.error
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
          <p class="text-[11px] text-ink-3">
            浅色为默认主题，深色适合夜间值守。
          </p>
        </div>
        <div class="grid grid-cols-2 gap-2.5 p-3.5">
          <button
            type="button"
            class="flex items-center gap-2.5 rounded-md border px-3 py-2.5 text-left transition-colors"
            :class="
              ui.theme === 'light'
                ? 'border-accent bg-accent-soft'
                : 'border-line hover:border-line-strong'
            "
            @click="ui.setTheme('light')"
          >
            <Sun :size="16" class="text-accent" />
            <span class="flex-1">
              <span class="block text-[12.5px] font-medium text-ink"
                >浅色模式</span
              >
              <span class="block text-[11px] text-ink-3">默认，日间使用</span>
            </span>
            <Check v-if="ui.theme === 'light'" :size="14" class="text-accent" />
          </button>

          <button
            type="button"
            class="flex items-center gap-2.5 rounded-md border px-3 py-2.5 text-left transition-colors"
            :class="
              ui.theme === 'dark'
                ? 'border-accent bg-accent-soft'
                : 'border-line hover:border-line-strong'
            "
            @click="ui.setTheme('dark')"
          >
            <Moon :size="16" class="text-accent" />
            <span class="flex-1">
              <span class="block text-[12.5px] font-medium text-ink"
                >深色模式</span
              >
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
          <span v-if="info?.encrypted" class="text-[10.5px] text-success">口令加密</span>
          <span v-else-if="info?.encrypted === false" class="text-[10.5px] text-ink-3">未加密</span>
          <span
            v-if="stats"
            class="ml-auto text-[11px] tabular-nums text-ink-3"
          >
            {{ formatBytes(stats.sizeBytes) }}
          </span>
        </div>

        <div v-if="loading" class="px-3.5 py-4 text-[11.5px] text-ink-3">
          正在读取数据库信息…
        </div>

        <div v-if="info?.encrypted === false" class="space-y-2 border-b border-line px-3.5 py-3">
          <p class="text-[11.5px] leading-relaxed text-ink-2">
            当前无需口令即可启动，数据库和备份以明文保存。可随时启用口令加密，现有事项会保留。
          </p>
          <button v-if="!encryptionFormOpen" type="button" class="btn h-8"
            :disabled="backingUp || restoring || restoreDialogOpen" @click="encryptionFormOpen = true">
            启用口令加密
          </button>
          <form v-else class="space-y-2" @submit.prevent="enableEncryption">
            <p class="text-[11.5px] leading-relaxed text-ink-3">
              启用后每次启动需要解锁；口令至少 12 个字符，忘记后无法找回。已有明文备份可在下方单独加密。
            </p>
            <label class="block text-[12px] text-ink-2">新口令
              <input v-model="newPassword" type="password" autocomplete="new-password" aria-label="设置新口令"
                class="input mt-1 w-full" :disabled="vault.busy" required />
            </label>
            <label class="block text-[12px] text-ink-2">再次输入口令
              <input v-model="confirmPassword" type="password" autocomplete="new-password" aria-label="确认设置口令"
                class="input mt-1 w-full" :disabled="vault.busy" required />
            </label>
            <p v-if="encryptionError" role="alert" class="text-[12px] text-p0">{{ encryptionError }}</p>
            <div class="flex gap-2">
              <button type="submit" class="btn btn-primary h-8" :disabled="vault.busy || backingUp || restoring || restoreDialogOpen">
                {{ vault.busy ? '正在加密…' : '确认启用加密' }}
              </button>
              <button type="button" class="btn h-8" :disabled="vault.busy" @click="cancelEncryption">取消</button>
            </div>
          </form>
        </div>

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
            <p class="mt-0.5 break-all font-mono text-[11.5px] text-ink-2">
              {{ stats.path }}
            </p>
          </div>

          <div class="grid grid-cols-2 gap-3 px-3.5 py-2.5 sm:grid-cols-4">
            <div>
              <div class="text-[10.5px] uppercase tracking-wide text-ink-3">
                事项总数
              </div>
              <div class="text-[13px] font-semibold tabular-nums text-ink">
                {{ stats.taskCount }}
              </div>
            </div>
            <div>
              <div class="text-[10.5px] uppercase tracking-wide text-ink-3">
                进展记录
              </div>
              <div class="text-[13px] font-semibold tabular-nums text-ink">
                {{ stats.updateCount }}
              </div>
            </div>
            <div>
              <div class="text-[10.5px] uppercase tracking-wide text-ink-3">
                结构版本
              </div>
              <div class="text-[13px] font-semibold tabular-nums text-ink">
                v{{ stats.schemaVersion }}
              </div>
            </div>
            <div>
              <div class="text-[10.5px] uppercase tracking-wide text-ink-3">
                日志模式
              </div>
              <div class="text-[13px] font-semibold text-ink">
                {{ stats.walEnabled ? 'WAL' : '缺省' }}
              </div>
            </div>
          </div>

          <div class="flex flex-wrap items-center gap-2 px-3.5 py-2.5">
            <button
              type="button"
              class="btn h-8"
              :disabled="backingUp || encryptingLegacyBackups || vault.busy"
              @click="backupNow"
            >
              <Save :size="13" />
              {{ backingUp ? '备份中…' : '立即备份' }}
            </button>
            <button
              type="button"
              class="btn h-8"
              :disabled="restoring || encryptingLegacyBackups || vault.busy"
              @click="restoreFromBackup"
            >
              <RotateCcw :size="13" />
              {{ restoring ? '恢复中…' : '从备份恢复' }}
            </button>
            <span class="text-[11px] text-ink-3">
              备份目录：<span class="font-mono">{{ backupDir }}</span>
            </span>
            <p
              v-if="lastBackupPath"
              class="w-full break-all font-mono text-[10.5px] text-success"
            >
              最近一次备份：{{ lastBackupPath }}
            </p>
            <p
              v-if="lastRestoreSnapshot"
              class="w-full break-all font-mono text-[10.5px] text-ink-3"
            >
              恢复前的数据已快照到：{{ lastRestoreSnapshot }}
            </p>
            <p class="w-full text-[10.5px] leading-relaxed text-ink-3">
              {{ info?.encrypted ? '备份以 .snbak 加密保存，可复制到其他电脑；恢复时需要原备份口令。请妥善保存口令，忘记后无法恢复。' : '备份以 .db 明文保存，可复制到其他电脑恢复，无需口令。' }}
            </p>
            <p class="w-full text-[10.5px] leading-relaxed text-ink-3">
              恢复会覆盖当前数据，覆盖前自动生成快照。{{ info?.encrypted ? '快照加密保存，关闭应用后数据会重新锁定。' : '快照也为明文；启用加密后可单独加密历史备份。' }}
            </p>
          </div>
        </div>

        <div v-if="info?.encrypted && plaintextBackupCount > 0" class="space-y-2 border-t border-line px-3.5 py-3">
          <p class="text-[12px] font-medium text-p1" role="status">
            仍有 {{ plaintextBackupCount }} 个旧版明文备份
          </p>
          <p class="text-[11.5px] leading-relaxed text-ink-2">
            可使用当前口令加密这些备份。加密成功后原明文文件会移除；失败时保留原文件，可稍后重试。
          </p>
          <button
            class="btn h-8"
            type="button"
            :disabled="encryptingLegacyBackups || backingUp || restoring"
            @click="encryptLegacyBackups"
          >
            {{ encryptingLegacyBackups ? '加密中…' : '加密旧备份' }}
          </button>
        </div>
      </div>

      <!-- 关于 -->
      <div class="rounded-lg border border-line bg-panel">
        <div class="border-b border-line px-3.5 py-2.5">
          <h2 class="text-[12.5px] font-semibold text-ink">关于 ShiftNote</h2>
        </div>
        <div class="space-y-1.5 px-3.5 py-3 text-[11.5px] text-ink-2">
          <p>版本 {{ info?.version ?? '—' }} · Tauri 2 + Vue 3 + SQLite</p>
          <p class="text-ink-3">
            轻量级交接班与事项跟踪工具。事项与交接记录保存在本地，可选口令加密，业务功能无需服务器。
          </p>
          <p class="text-[11.5px] leading-relaxed text-ink-3">
            WebView2 运行时可能向 Microsoft 发送诊断与崩溃报告；本地数据加密不保护解锁后的进程内存或系统转储。
          </p>
          <p v-if="info" class="text-ink-3">数据目录：{{ info.dataDir }}</p>
          <p v-if="info?.recoveredBackup" class="text-p0">
            上次启动检测到数据库损坏，已自动备份并重建：{{
              info.recoveredBackup
            }}
          </p>
        </div>
      </div>
    </div>
    <Teleport to="body">
    <div
      v-if="restoreDialogOpen"
      class="fixed inset-0 z-40 flex items-center justify-center bg-black/35 p-6"
      @click.self="cancelRestore"
      @keydown.esc.prevent="cancelRestore"
    >
      <form
        class="w-full max-w-md space-y-3 rounded-xl border border-line bg-panel p-4 text-ink shadow-[var(--sn-shadow-pop)]"
        role="dialog"
        aria-modal="true"
        aria-labelledby="restore-title"
        @submit.prevent="restoreSelectedBackup"
      >
        <h2 id="restore-title" class="text-[13.5px] font-semibold">从备份恢复数据？</h2>
        <p class="text-[12px] leading-relaxed text-ink-2">
          当前数据会被备份文件覆盖。覆盖前会自动生成{{ info?.encrypted ? '加密' : '明文' }}快照，必要时可用它回退。
        </p>
        <p class="break-all font-mono text-[11px] text-ink-3">{{ selectedBackupPath }}</p>
        <label class="block text-[12px] text-ink-2">
          原备份口令（可选）
          <input
            v-model="restorePassword"
            class="input mt-1 w-full"
            type="password"
            autocomplete="off"
            aria-label="原备份口令"
            :disabled="restoring"
            spellcheck="false"
            autocapitalize="off"
            autofocus
          />
        </label>
        <p class="text-[11.5px] leading-relaxed text-ink-3">
          明文 .db 备份无需口令。来自其他电脑或其他安装的加密备份，即使口令相同，也请填写生成备份时的口令。恢复后保持当前是否启用加密的选择。
        </p>
        <p v-if="restoreError" class="text-[12px] text-p0" role="alert">{{ restoreError }}</p>
        <div class="flex justify-end gap-2 pt-1">
          <button class="btn h-8" type="button" :disabled="restoring" @click="cancelRestore">取消</button>
          <button class="btn btn-danger h-8" type="submit" :disabled="restoring">
            {{ restoring ? '恢复中…' : '开始恢复' }}
          </button>
        </div>
      </form>
    </div>
    </Teleport>
  </section>
</template>
