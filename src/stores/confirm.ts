import { defineStore } from 'pinia'
import { ref } from 'vue'

export interface ConfirmOptions {
  title: string
  message?: string
  confirmText?: string
  cancelText?: string
  danger?: boolean
}

interface ConfirmState extends ConfirmOptions {
  open: boolean
}

export const useConfirmStore = defineStore('confirm', () => {
  const state = ref<ConfirmState>({
    open: false,
    title: '',
    message: '',
    confirmText: '确定',
    cancelText: '取消',
    danger: false,
  })

  let resolver: ((value: boolean) => void) | null = null

  /** 返回 Promise<boolean>，用户确认后为 true。 */
  function ask(options: ConfirmOptions): Promise<boolean> {
    // 上一次未决的确认框直接按“取消”处理，避免 Promise 悬挂
    resolver?.(false)
    state.value = {
      open: true,
      title: options.title,
      message: options.message ?? '',
      confirmText: options.confirmText ?? '确定',
      cancelText: options.cancelText ?? '取消',
      danger: options.danger ?? false,
    }
    return new Promise<boolean>((resolve) => {
      resolver = resolve
    })
  }

  function settle(value: boolean) {
    state.value.open = false
    const current = resolver
    resolver = null
    current?.(value)
  }

  const accept = () => settle(true)
  const cancel = () => settle(false)

  return { state, ask, accept, cancel }
})
