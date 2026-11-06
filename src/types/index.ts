/// 与 Rust 端 `models.rs` 一一对应的数据结构（serde camelCase）。

export type TaskStatus = 'open' | 'done'

export type ViewKey =
  | 'all'
  | 'today'
  | 'open'
  | 'focus'
  | 'done'
  | 'hidden'
  | 'archived'
  | 'trash'
  | 'search'

export type SortKey = 'priority' | 'recorded' | 'created' | 'updated' | 'manual'

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
  archived: number
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
  /** 兼容旧版浏览器测试 fixture；生产由后端返回。 */
  encrypted?: boolean
  plaintextBackupCount?: number
}

export interface VaultStatus {
  configured: boolean
  unlocked: boolean
  legacy: boolean
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

export interface LegacyBackupEncryptionSummary {
  encrypted: number
  remaining: number
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
  invalidIds?: string[]
}

export interface TaskPage {
  items: Task[]
  total: number
  limit: number
  offset: number
}

export type TaskBatchAction =
  | 'complete'
  | 'reopen'
  | 'hide'
  | 'unhide'
  | 'priority'
  | 'trash'
  | 'restore'
  | 'archive'
  | 'unarchive'
  | 'deletePermanent'
export interface TaskBatchInput {
  ids: string[]
  action: TaskBatchAction
  priority?: number
}
export interface TaskBatchResult {
  requested: number
  changed: number
  unchanged: number
}
export interface TaskMoveInput {
  id: string
  beforeId?: string | null
  afterId?: string | null
  view: ViewKey
}

export type HandoverSection =
  'new' | 'handled' | 'pending' | 'follow_up' | 'focus'
export type HandoverFormat = 'markdown' | 'txt'
export interface Handover {
  id: string
  title: string
  handoverDate: string
  shiftFrom: string | null
  shiftTo: string | null
  note: string | null
  periodStart: number
  periodEnd: number
  status: 'draft' | 'finalized'
  finalizedAt: number | null
  snapshotVersion: number
  createdAt: number
  updatedAt: number
}
export interface TaskSnapshot {
  task: Task
  updates: TaskUpdate[]
}
export interface HandoverItem {
  id: string
  taskId: string | null
  section: HandoverSection
  note: string | null
  sortOrder: number
  selectedUpdateIds: string[]
  snapshot: TaskSnapshot
}
export interface HandoverDetail {
  handover: Handover
  items: HandoverItem[]
}
export interface HandoverPage {
  items: Handover[]
  total: number
  limit: number
  offset: number
}
export interface HandoverItemInput {
  taskId: string
  section: HandoverSection
  note: string | null
  selectedUpdateIds: string[]
}
export interface HandoverSaveInput {
  id?: string | null
  title: string
  periodStart: number
  periodEnd: number
  shiftFrom: string | null
  shiftTo: string | null
  note: string | null
  items: HandoverItemInput[]
}
export interface HandoverFilter {
  search?: string | null
  status?: string | null
  limit?: number
  offset?: number
}
export interface HandoverCandidateFilter {
  periodStart: number
  periodEnd: number
  kind: 'all' | 'new' | 'handled' | 'pending' | 'progress'
  search?: string | null
  limit?: number
  offset?: number
}
export interface HandoverExportResult {
  path: string
  bytes: number
}

export interface UpdateInfo {
  currentVersion: string
  latestVersion: string | null
  hasUpdate: boolean
  releaseName: string | null
  releaseUrl: string | null
  publishedAt: string | null
  notes: string | null
  checkedAt: number
  usedProxy: boolean
}
