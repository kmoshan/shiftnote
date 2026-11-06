<script setup lang="ts">
import { computed } from 'vue'
import { CheckCircle2, EyeOff, Inbox, Search, Trash2 } from '@lucide/vue'
import type { Component } from 'vue'

const props = defineProps<{
  view: string
  searching: boolean
}>()

interface EmptyCopy {
  icon: Component
  title: string
  description: string
}

const copy = computed<EmptyCopy>(() => {
  if (props.searching) {
    return {
      icon: Search,
      title: '没有匹配的事项',
      description: '换个关键词试试；搜索会同时匹配历史进展内容。',
    }
  }
  switch (props.view) {
    case 'today':
      return { icon: Inbox, title: '今天还没有新建事项', description: '用上方输入框快速记录今天要跟进的事情。' }
    case 'open':
      return { icon: CheckCircle2, title: '没有待处理事项', description: '所有事项都已完成，保持这个节奏。' }
    case 'focus':
      return { icon: Inbox, title: '没有重点事项', description: 'P0 紧急与 P1 重要的事项会集中出现在这里。' }
    case 'done':
      return { icon: CheckCircle2, title: '还没有已完成的事项', description: '勾选事项左侧的方框即可标记完成。' }
    case 'hidden':
      return { icon: EyeOff, title: '没有隐藏的事项', description: '在事项菜单中选择「隐藏」即可从列表移除但保留记录。' }
    case 'trash':
      return { icon: Trash2, title: '回收站是空的', description: '移入回收站的事项会保留在这里，可随时恢复或永久删除。' }
    default:
      return { icon: Inbox, title: '还没有事项', description: '在顶部输入框中输入标题并回车，即可创建第一条交接事项。' }
  }
})
</script>

<template>
  <div class="flex flex-col items-center justify-center gap-2 px-6 py-16 text-center">
    <div class="grid h-11 w-11 place-items-center rounded-full bg-panel-2 text-ink-3">
      <component :is="copy.icon" :size="20" :stroke-width="1.8" />
    </div>
    <p class="text-[13px] font-medium text-ink-2">{{ copy.title }}</p>
    <p class="max-w-[320px] text-[11.5px] leading-relaxed text-ink-3">{{ copy.description }}</p>
  </div>
</template>
