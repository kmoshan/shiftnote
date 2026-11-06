import { onMounted, onUnmounted, ref } from 'vue'

/** 定时刷新的“当前时间”，用于顶栏时钟与相对时间显示。 */
export function useNow(intervalMs = 20000) {
  const now = ref(Date.now())
  let timer: number | undefined

  onMounted(() => {
    now.value = Date.now()
    timer = window.setInterval(() => {
      now.value = Date.now()
    }, intervalMs)
  })

  onUnmounted(() => {
    if (timer !== undefined) window.clearInterval(timer)
  })

  return now
}
