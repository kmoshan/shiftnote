/// 与 Rust 端 `models.rs` 一一对应的数据结构（serde camelCase）。

export type TaskStatus = 'open' | 'done'

export type ViewKey =
  | 'all'
  | 'today'
  | 'open'
  | 'focus'
  | 'done'
  | 'hidden'
  | 'trash'
  | 'search'

export type SortKey = 'priority' | 'recorded' | 'created' | 'updated'

export interface Task {
  id: string
  title: string
  description: string | null
  priority: number
  status: TaskStatus
  completedAt: number | null
  hidden: boolean
  archived: boolean
  deletedAt: number | null
  categoryId: string | null
  /** 负责部门（由旧版负责人 / 记录人 / 交接对象合并而来） */
  department: string | null
  /** 记录时间：可手工指定，列表按它筛选日期 */
  recordedAt: number
  sortOrder: number
  createdAt: number
  updatedAt: number
  latestUpdateContent: string | null
  latestUpdateAt: number | null
  updateCount: number
  reminderEnabled: boolean
  nextRemindAt: number | null
}

export interface TaskUpdate {
  id: string
  taskId: string
  content: string
  author: string | null
  kind: 'progress' | 'system'
  createdAt: number
  updatedAt: number
  editedAt: number | null
}

export interface TaskDetail {
  task: Task
  updates: TaskUpdate[]
}

export interface Counts {
  all: number
  today: number
  open: number
  focus: number
  done: number
  hidden: number
  trash: number
}

export interface CreateTaskInput {
  title: string
  description?: string | null
  priority?: number
  department?: string | null
  categoryId?: string | null
  /** 记录时间（毫秒时间戳），缺省为当前时间 */
  recordedAt?: number | null
}

export interface TaskPatch {
  title?: string
  description?: string
  priority?: number
  department?: string
  categoryId?: string
  recordedAt?: number
  sortOrder?: number
}

export interface TaskFilter {
  view?: ViewKey
  priority?: number | null
  search?: string | null
  sort?: SortKey
  limit?: number
  offset?: number
  dateFrom?: number | null
  dateTo?: number | null
  categoryId?: string | null
}

export interface AppInfo {
  version: string
  dataDir: string
  dbPath: string
  schemaVersion: number
  recoveredBackup: string | null
}

export interface DbStats {
  path: string
  sizeBytes: number
  taskCount: number
  updateCount: number
  schemaVersion: number
  walEnabled: boolean
}

export interface BackupInfo {
  path: string
  sizeBytes: number
  createdAt: number
}

export interface RestoreSummary {
  sourcePath: string
  /** 恢复前自动生成的当前数据快照，可用于回退 */
  snapshotPath: string
  taskCount: number
  updateCount: number
  schemaVersion: number
}

export interface ApiError {
  code?: string
  message: string
}
