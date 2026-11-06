export interface PriorityMeta {
  value: number
  code: string
  label: string
  /** 徽标样式：颜色 + 文字双重标识，不只依赖颜色 */
  chipClass: string
  textClass: string
  dotClass: string
  /** 默认提醒间隔（分钟），0 表示默认不提醒 */
  defaultInterval: number
}

export const PRIORITIES: PriorityMeta[] = [
  {
    value: 0,
    code: 'P0',
    label: '紧急',
    chipClass: 'chip-p0',
    textClass: 'text-p0',
    dotClass: 'bg-p0',
    defaultInterval: 30,
  },
  {
    value: 1,
    code: 'P1',
    label: '重要',
    chipClass: 'chip-p1',
    textClass: 'text-p1',
    dotClass: 'bg-p1',
    defaultInterval: 120,
  },
  {
    value: 2,
    code: 'P2',
    label: '普通',
    chipClass: 'chip-p2',
    textClass: 'text-p2',
    dotClass: 'bg-p2',
    defaultInterval: 0,
  },
  {
    value: 3,
    code: 'P3',
    label: '低',
    chipClass: 'chip-p3',
    textClass: 'text-p3',
    dotClass: 'bg-p3',
    defaultInterval: 0,
  },
]

export function priorityMeta(value: number): PriorityMeta {
  return PRIORITIES[value] ?? PRIORITIES[2]
}

export const VIEW_LABELS: Record<string, string> = {
  all: '全部事项',
  today: '今日事项',
  open: '待处理事项',
  focus: '重点事项',
  done: '已完成事项',
  hidden: '已隐藏事项',
  archived: '已归档事项',
  trash: '回收站',
  search: '搜索结果',
}

export const VIEW_HINTS: Record<string, string> = {
  all: '正在跟进的全部事项，按优先级从高到低排列',
  today: '记录时间落在今天的事项',
  open: '还没有完成的事项',
  focus: 'P0 紧急与 P1 重要的未完成事项',
  done: '已完成的事项，可取消完成或隐藏',
  hidden: '从列表隐藏的事项，记录仍然保留',
  archived: '已完成并归档的事项，可取消归档或取消完成',
  trash: '已移入回收站的事项，可恢复或永久删除',
  search: '同时匹配标题、描述、负责部门与全部历史进展',
}
