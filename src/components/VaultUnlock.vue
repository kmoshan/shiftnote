<script setup lang="ts">
import { computed, ref } from 'vue'

import { useVaultStore } from '@/stores/vault'

const emit = defineEmits<{ unlocked: [] }>()
const vault = useVaultStore()
const password = ref('')
const confirmation = ref('')
const migrationAccepted = ref(false)
const validationError = ref('')
const settingUp = computed(() => !vault.configured)

async function submit() {
  if (vault.busy || !vault.checked) return
  validationError.value = ''
  if (!password.value) {
    validationError.value = '请输入口令'
    return
  }
  if (settingUp.value && Array.from(password.value).length < 12) {
    validationError.value = '新口令至少需要 12 个字符'
    password.value = ''
    confirmation.value = ''
    return
  }
  if (settingUp.value && password.value !== confirmation.value) {
    validationError.value = '两次输入的口令不一致，请重新输入'
    password.value = ''
    confirmation.value = ''
    return
  }
  if (vault.legacy && !migrationAccepted.value) {
    validationError.value = '请先确认加密迁移现有数据'
    return
  }

  const submittedPassword = password.value
  const submittedConfirmation = settingUp.value
    ? confirmation.value
    : undefined
  password.value = ''
  confirmation.value = ''
  if (
    await vault.unlock(
      submittedPassword,
      submittedConfirmation,
      vault.legacy && migrationAccepted.value
    )
  ) {
    emit('unlocked')
  }
}

async function skip() {
  password.value = ''
  confirmation.value = ''
  validationError.value = ''
  if (await vault.skip()) emit('unlocked')
}
</script>

<template>
  <main class="flex h-full items-center justify-center overflow-y-auto bg-canvas p-6 text-ink">
    <section
      class="w-full max-w-md rounded-xl border border-line bg-panel p-6 shadow-sm"
      aria-labelledby="vault-title"
    >
      <p class="mb-1 text-[11.5px] font-medium text-accent">ShiftNote</p>
      <h1 id="vault-title" class="text-base font-semibold">
        {{ vault.checked && settingUp ? '设置数据加密口令' : '解锁本地数据' }}
      </h1>

      <p v-if="vault.checking" class="mt-3 text-[12.5px] text-ink-2" role="status">
        正在检查本地加密状态…
      </p>
      <template v-else-if="!vault.checked">
        <p class="mt-3 text-[12.5px] text-p0" role="alert">
          {{ vault.error || '无法检查本地数据状态，请重试' }}
        </p>
        <button class="btn mt-4 h-8" type="button" @click="vault.initialize()">
          重试
        </button>
      </template>
      <form v-else class="mt-3 space-y-3" @submit.prevent="submit">
        <p class="text-[12.5px] leading-relaxed text-ink-2">
          {{
            settingUp
              ? '口令加密可选，也可先跳过，之后在设置中启用。启用后每次启动需要解锁，新口令至少 12 个字符。'
              : '请输入数据加密口令，继续查看事项和交接记录。'
          }}
        </p>
        <p
          v-if="settingUp"
          class="rounded-md bg-panel-2 p-2.5 text-[11.5px] leading-relaxed text-ink-2"
        >
          ShiftNote 不提供口令找回。忘记口令将无法恢复数据；在其他电脑恢复加密备份时，也需要原备份的口令。
        </p>
        <label class="block text-[12px] text-ink-2">
          {{ settingUp ? '新口令' : '加密口令' }}
          <input
            v-model="password"
            class="input mt-1 w-full"
            type="password"
            :autocomplete="settingUp ? 'new-password' : 'current-password'"
            :aria-label="settingUp ? '新口令' : '加密口令'"
            :disabled="vault.busy"
            spellcheck="false"
            autocapitalize="off"
            required
            autofocus
          />
        </label>
        <label v-if="settingUp" class="block text-[12px] text-ink-2">
          再次输入口令
          <input
            v-model="confirmation"
            class="input mt-1 w-full"
            type="password"
            autocomplete="new-password"
            aria-label="再次输入口令"
            :disabled="vault.busy"
            spellcheck="false"
            autocapitalize="off"
            required
          />
        </label>
        <label
          v-if="vault.legacy"
          class="flex items-start gap-2 rounded-md border border-line p-2.5 text-[11.5px] leading-relaxed text-ink-2"
        >
          <input
            v-model="migrationAccepted"
            class="mt-0.5"
            type="checkbox"
            :disabled="vault.busy"
          />
          <span>检测到旧版未加密数据。我确认将现有数据库迁移为口令加密格式，并会妥善保存口令。</span>
        </label>
        <p v-if="validationError || vault.error" class="text-[12px] text-p0" role="alert">
          {{ validationError || vault.error }}
        </p>
        <button class="btn btn-primary h-9 w-full" type="submit" :disabled="vault.busy">
          {{ vault.busy ? '正在解锁…' : settingUp ? '设置口令并解锁' : '解锁' }}
        </button>
        <template v-if="settingUp">
          <button class="btn h-9 w-full" type="button" :disabled="vault.busy" @click="skip">
            暂不加密，直接进入
          </button>
          <p class="text-[11.5px] leading-relaxed text-ink-3">
            跳过后数据库和备份以明文保存，后续启动不再提示；可在设置中启用口令加密。
          </p>
        </template>
        <p v-if="!settingUp" class="text-[11.5px] leading-relaxed text-ink-3">
          忘记口令将无法恢复数据，ShiftNote 不提供口令找回。
        </p>
      </form>
    </section>
  </main>
</template>
