import { defineStore } from 'pinia'
import { ref } from 'vue'

export type ToastKind = 'success' | 'error' | 'info'

export interface ToastItem {
  id: number
  message: string
  kind: ToastKind
}

let seed = 0

export const useToastStore = defineStore('toast', () => {
  const items = ref<ToastItem[]>([])

  function dismiss(id: number) {
    items.value = items.value.filter((item) => item.id !== id)
  }

  function push(message: string, kind: ToastKind = 'info', timeout = 3600) {
    const id = ++seed
    items.value = [...items.value.slice(-3), { id, message, kind }]
    if (timeout > 0) {
      window.setTimeout(() => dismiss(id), timeout)
    }
    return id
  }

  const success = (message: string) => push(message, 'success', 2600)
  const error = (message: string) => push(message, 'error', 5200)
  const info = (message: string) => push(message, 'info', 3000)

  return { items, push, dismiss, success, error, info }
})
