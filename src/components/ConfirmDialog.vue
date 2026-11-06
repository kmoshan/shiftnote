<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { AlertTriangle } from '@lucide/vue'

import { useConfirmStore } from '@/stores/confirm'

const confirm = useConfirmStore()

function onKeydown(event: KeyboardEvent) {
  if (!confirm.state.open) return
  if (event.key === 'Escape') {
    event.preventDefault()
    confirm.cancel()
  }
}

onMounted(() => window.addEventListener('keydown', onKeydown, true))
onUnmounted(() => window.removeEventListener('keydown', onKeydown, true))
</script>

<template>
  <Teleport to="body">
    <div
      v-if="confirm.state.open"
      class="fixed inset-0 z-40 flex items-center justify-center bg-black/35 p-6 animate-fade-in"
      @click.self="confirm.cancel()"
    >
      <div class="w-[380px] max-w-full rounded-xl border border-line bg-panel p-4 shadow-[var(--sn-shadow-pop)] animate-pop-in">
        <div class="flex items-start gap-2.5">
          <div
            class="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-full"
            :class="confirm.state.danger ? 'bg-p0-soft text-p0' : 'bg-accent-soft text-accent'"
          >
            <AlertTriangle :size="15" />
          </div>
          <div class="min-w-0 flex-1">
            <h2 class="text-[13.5px] font-semibold text-ink">{{ confirm.state.title }}</h2>
            <p v-if="confirm.state.message" class="mt-1 text-[12.5px] leading-relaxed text-ink-2">
              {{ confirm.state.message }}
            </p>
          </div>
        </div>

        <div class="mt-4 flex justify-end gap-2">
          <button class="btn h-8" type="button" @click="confirm.cancel()">
            {{ confirm.state.cancelText }}
          </button>
          <button
            class="btn h-8"
            :class="confirm.state.danger ? 'btn-danger border-[color:var(--sn-p0)]/40' : 'btn-primary'"
            type="button"
            autofocus
            @click="confirm.accept()"
          >
            {{ confirm.state.confirmText }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
