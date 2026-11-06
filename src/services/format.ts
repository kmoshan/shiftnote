/// 时间与文本格式化工具。数据库存储 Unix 毫秒时间戳（UTC），此处统一转本地时区展示。

const pad = (n: number) => String(n).padStart(2, '0')

export function formatClock(ms: number): string {
  const d = new Date(ms)
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export function formatDate(ms: number): string {
  const d = new Date(ms)
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

export function formatDateTime(ms: number): string {
  return `${formatDate(ms)} ${formatClock(ms)}`
}

export function formatMonthDay(ms: number): string {
  const d = new Date(ms)
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

export function isSameDay(a: number, b: number): boolean {
  const da = new Date(a)
  const db = new Date(b)
  return (
    da.getFullYear() === db.getFullYear() &&
    da.getMonth() === db.getMonth() &&
    da.getDate() === db.getDate()
  )
}

/** 今天 14:05 / 昨天 09:12 / 11-03 18:20 / 2025-11-03 18:20 */
export function formatSmart(ms: number, now = Date.now()): string {
  const day = 24 * 60 * 60 * 1000
  if (isSameDay(ms, now)) return `今天 ${formatClock(ms)}`
  if (isSameDay(ms, now - day)) return `昨天 ${formatClock(ms)}`
  const d = new Date(ms)
  const n = new Date(now)
  if (d.getFullYear() === n.getFullYear()) return `${formatMonthDay(ms)} ${formatClock(ms)}`
  return formatDateTime(ms)
}

/** 刚刚 / 5 分钟前 / 3 小时前 / 2 天前 */
export function formatRelative(ms: number, now = Date.now()): string {
  const diff = now - ms
  const minute = 60 * 1000
  const hour = 60 * minute
  const day = 24 * hour

  if (diff < 0) return formatSmart(ms, now)
  if (diff < minute) return '刚刚'
  if (diff < hour) return `${Math.floor(diff / minute)} 分钟前`
  if (diff < day) return `${Math.floor(diff / hour)} 小时前`
  if (diff < 7 * day) return `${Math.floor(diff / day)} 天前`
  return formatDate(ms)
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`
}

/** 用于 <input type="datetime-local"> 的本地时间字符串 */
export function toDatetimeInput(ms: number | null): string {
  if (!ms) return ''
  const d = new Date(ms)
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(
    d.getHours()
  )}:${pad(d.getMinutes())}`
}

/** datetime-local 字符串（本地时区）转毫秒时间戳 */
export function fromDatetimeInput(value: string): number | null {
  if (!value) return null
  const ts = new Date(value).getTime()
  return Number.isNaN(ts) ? null : ts
}

/**
 * 把 `YYYY-MM-DD`（本地日期）转成半开区间 [当天 00:00, 次日 00:00) 的毫秒时间戳。
 * 用本地 Date 构造，可正确处理夏令时与月末。
 */
export function localDayBounds(date: string): { from: number; to: number } | null {
  const matched = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date.trim())
  if (!matched) return null
  const year = Number(matched[1])
  const month = Number(matched[2])
  const day = Number(matched[3])
  const from = new Date(year, month - 1, day).getTime()
  const to = new Date(year, month - 1, day + 1).getTime()
  if (Number.isNaN(from) || Number.isNaN(to)) return null
  return { from, to }
}

/** 时间戳 → 本地 `YYYY-MM-DD`，用于日期选择器 */
export function toDateInput(ms: number): string {
  const d = new Date(ms)
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

/** 相对今天 days 天前的本地日期（0=今天，1=昨天），返回 YYYY-MM-DD */
export function dateDaysAgo(days: number): string {
  const now = new Date()
  const target = new Date(now.getFullYear(), now.getMonth(), now.getDate() - days)
  return toDateInput(target.getTime())
}

export function truncate(text: string, max: number): string {
  const chars = Array.from(text)
  return chars.length <= max ? text : `${chars.slice(0, max).join('')}…`
}

export function pluralCount(n: number, unit = '条'): string {
  return `${n} ${unit}`
}
