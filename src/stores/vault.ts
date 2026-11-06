import { defineStore } from 'pinia'
import { ref } from 'vue'

import { api, toApiFailure } from '@/services/api'

/** 只保存解锁状态；口令仅作为单次 IPC 调用的参数传递。 */
export const useVaultStore = defineStore('vault', () => {
  const configured = ref(false)
  const unlocked = ref(false)
  const legacy = ref(false)
  const checked = ref(false)
  const checking = ref(false)
  const busy = ref(false)
  const error = ref('')

  async function initialize() {
    if (checking.value) return
    checking.value = true
    error.value = ''
    try {
      const status = await api.vaultStatus()
      configured.value = status.configured
      unlocked.value = status.unlocked
      legacy.value = status.legacy
      checked.value = true
    } catch (caught) {
      error.value = toApiFailure(caught).message
      checked.value = false
      unlocked.value = false
    } finally {
      checking.value = false
    }
  }

  async function unlock(
    password: string,
    confirmPassword?: string,
    migrateLegacy = false
  ) {
    if (busy.value || !checked.value) return false
    busy.value = true
    error.value = ''
    try {
      await api.vaultUnlock(password, confirmPassword, migrateLegacy)
      configured.value = true
      unlocked.value = true
      legacy.value = false
      return true
    } catch (caught) {
      error.value = toApiFailure(caught).message
      return false
    } finally {
      busy.value = false
    }
  }

  async function skip() {
    if (busy.value || !checked.value || configured.value) return false
    busy.value = true
    error.value = ''
    try {
      await api.vaultSkip()
      configured.value = false
      unlocked.value = true
      legacy.value = false
      return true
    } catch (caught) {
      error.value = toApiFailure(caught).message
      return false
    } finally {
      busy.value = false
    }
  }

  async function enableEncryption(password: string, confirmation: string) {
    if (busy.value || !unlocked.value || configured.value) return false
    busy.value = true
    error.value = ''
    try {
      await api.vaultEnableEncryption(password, confirmation)
      configured.value = true
      return true
    } catch (caught) {
      error.value = toApiFailure(caught).message
      return false
    } finally {
      busy.value = false
    }
  }

  return {
    configured,
    unlocked,
    legacy,
    checked,
    checking,
    busy,
    error,
    initialize,
    unlock,
    skip,
    enableEncryption,
  }
})
