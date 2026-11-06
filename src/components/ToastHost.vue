<script setup lang="ts">
import { AlertTriangle, CheckCircle2, Info, X } from '@lucide/vue'

import { useToastStore } from '@/stores/toast'

const toast = useToastStore()

const styles: Record<string, { box: string; icon: typeof Info }> = {
  success: { box: 'border-[color:var(--sn-success)]/30 text-ink', icon: CheckCircle2 },
  error: { box: 'border-[color:var(--sn-p0)]/40 text-ink', icon: AlertTriangle },
  info: { box: 'border-line text-ink', icon: Info },
}
</script>

<template>
  <div class="pointer-events-none fixed bottom-3 right-3 z-50 flex w-[320px] flex-col gap-2">
    <TransitionGroup name="toast">
      <div
        v-for="item in toast.items"
        :key="item.id"
        class="pointer-events-auto flex items-start gap-2 rounded-lg border bg-panel px-3 py-2 shadow-[var(--sn-shadow-pop)]"
        :class="(styles[item.kind] ?? styles.info).box"
      >
        <component
          :is="(styles[item.kind] ?? styles.info).icon"
          :size="15"
          class="mt-[1px] shrink-0"
          :class="
            item.kind === 'success'
              ? 'text-[color:var(--sn-success)]'
              : item.kind === 'error'
                ? 'text-p0'
                : 'text-accent'
          "
        />
        <p class="flex-1 text-[12.5px] leading-relaxed">{{ item.message }}</p>
        <button
          type="button"
          class="mt-[1px] shrink-0 rounded p-0.5 text-ink-3 hover:bg-hover hover:text-ink"
          @click="toast.dismiss(item.id)"
        >
          <X :size="13" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: opacity 0.16s ease, transform 0.16s ease;
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(6px);
}
</style>
