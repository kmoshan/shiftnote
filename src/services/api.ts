import { invoke } from '@tauri-apps/api/core'

import type {
  AppInfo,
  BackupInfo,
  LegacyBackupEncryptionSummary,
  Counts,
  CreateTaskInput,
  DbStats,
  RestoreSummary,
  Task,
  TaskDetail,
  TaskFilter,
  TaskPatch,
  TaskUpdate,
  TaskPage,
  TaskBatchInput,
  TaskBatchResult,
  TaskMoveInput,
  HandoverDetail,
  HandoverPage,
  HandoverSaveInput,
  HandoverFilter,
  HandoverCandidateFilter,
  HandoverFormat,
  HandoverExportResult,
  VaultStatus,
} from '@/types'

/** 统一的调用失败对象：后端 AppError 会被序列化为 { code, message }。 */
export class ApiFailure extends Error {
  readonly code: string
  readonly invalidIds: string[]

  constructor(message: string, code = 'unknown', invalidIds: string[] = []) {
    super(message)
    this.name = 'ApiFailure'
    this.code = code
    this.invalidIds = invalidIds
  }
}

export function toApiFailure(error: unknown): ApiFailure {
  if (error instanceof ApiFailure) return error
  if (typeof error === 'string') return new ApiFailure(error)
  if (error && typeof error === 'object') {
    const candidate = error as {
      message?: unknown
      code?: unknown
      invalidIds?: unknown
    }
    if (typeof candidate.message === 'string') {
      return new ApiFailure(
        candidate.message,
        typeof candidate.code === 'string' ? candidate.code : 'unknown',
        Array.isArray(candidate.invalidIds)
          ? candidate.invalidIds.filter(
              (id): id is string => typeof id === 'string'
            )
          : []
      )
    }
    if (error instanceof Error) return new ApiFailure(error.message)
  }
  return new ApiFailure('操作失败，请稍后重试')
}

async function call<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  try {
    return await invoke<T>(command, args)
  } catch (error) {
    throw toApiFailure(error)
  }
}

/** 是否运行在 Tauri 容器内（浏览器直接打开时没有 IPC）。 */
export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/**
 * 与 Rust 命令层一一对应的调用封装。
 * 这里没有任何模拟数据：所有读写都真正落到本地 SQLite。
 */
export const api = {
  vaultStatus: () => call<VaultStatus>('vault_status'),
  vaultUnlock: (
    password: string,
    confirmPassword?: string,
    migrateLegacy = false
  ) => call<void>('vault_unlock', { password, confirmPassword, migrateLegacy }),
  vaultSkip: () => call<void>('vault_skip'),
  vaultEnableEncryption: (password: string, confirmPassword: string) =>
    call<void>('vault_enable_encryption', { password, confirmPassword }),
  appInfo: () => call<AppInfo>('app_info'),
  dbStats: () => call<DbStats>('db_stats'),
  dbBackup: () => call<BackupInfo>('db_backup'),
  encryptLegacyBackups: () =>
    call<LegacyBackupEncryptionSummary>('db_encrypt_legacy_backups'),
  dbRestore: (path: string, backupPassword?: string) =>
    call<RestoreSummary>('db_restore', { path, backupPassword }),

  settingsAll: () => call<Record<string, string>>('settings_all'),
  settingsGet: (key: string) => call<string | null>('settings_get', { key }),
  settingsSet: (key: string, value: string) =>
    call<void>('settings_set', { key, value }),

  listTasks: (filter: TaskFilter) => call<Task[]>('task_list', { filter }),
  pageTasks: (filter: TaskFilter) => call<TaskPage>('task_page', { filter }),
  batchTasks: (input: TaskBatchInput) =>
    call<TaskBatchResult>('task_batch', { input }),
  moveTask: (input: TaskMoveInput) => call<void>('task_move', { input }),
  setArchived: (id: string, archived: boolean) =>
    call<Task>('task_set_archived', { id, archived }),
  getTask: (id: string) => call<TaskDetail>('task_get', { id }),
  createTask: (input: CreateTaskInput) => call<Task>('task_create', { input }),
  updateTask: (id: string, patch: TaskPatch) =>
    call<Task>('task_update', { id, patch }),
  setCompleted: (id: string, completed: boolean) =>
    call<Task>('task_set_completed', { id, completed }),
  setHidden: (id: string, hidden: boolean) =>
    call<Task>('task_set_hidden', { id, hidden }),
  setPriority: (id: string, priority: number) =>
    call<Task>('task_set_priority', { id, priority }),
  trashTask: (id: string) => call<void>('task_trash', { id }),
  restoreTask: (id: string) => call<Task>('task_restore', { id }),
  deleteTaskPermanent: (id: string) =>
    call<void>('task_delete_permanent', { id }),
  duplicateTask: (id: string) => call<Task>('task_duplicate', { id }),
  reorderTasks: (orderedIds: string[]) =>
    call<void>('task_reorder', { orderedIds }),
  counts: () => call<Counts>('task_counts'),

  listUpdates: (taskId: string) =>
    call<TaskUpdate[]>('update_list', { taskId }),
  addUpdate: (taskId: string, content: string, author?: string | null) =>
    call<TaskUpdate>('update_add', { taskId, content, author: author ?? null }),
  editUpdate: (id: string, content: string) =>
    call<TaskUpdate>('update_edit', { id, content }),
  deleteUpdate: (id: string) => call<void>('update_delete', { id }),

  listHandovers: (filter: HandoverFilter = {}) =>
    call<HandoverPage>('handover_list', { filter }),
  getHandover: (id: string) => call<HandoverDetail>('handover_get', { id }),
  saveHandover: (input: HandoverSaveInput) =>
    call<HandoverDetail>('handover_save', { input }),
  handoverCandidates: (filter: HandoverCandidateFilter) =>
    call<TaskPage>('handover_candidates', { filter }),
  finalizeHandover: (id: string) =>
    call<HandoverDetail>('handover_finalize', { id }),
  duplicateHandover: (id: string) =>
    call<HandoverDetail>('handover_duplicate', { id }),
  deleteHandover: (id: string) => call<void>('handover_delete', { id }),
  renderHandover: (id: string, format: HandoverFormat) =>
    call<string>('handover_render', { id, format }),
  exportHandover: (
    id: string,
    format: HandoverFormat,
    path: string,
    overwrite = false
  ) =>
    call<HandoverExportResult>('handover_export', {
      id,
      format,
      path,
      overwrite,
    }),
}
