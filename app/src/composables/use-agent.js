import { useAgent as useAgentBase } from '@7n/tauri-components/vue'
import { TOOLS } from '../tool/catalog.js'
import { systemPrompt } from '../tool/prompt.js'

/** @returns {object} in-app чат-агент myllm через локальний omlx (без доменних інструментів) */
export function useAgent() {
  return useAgentBase({
    catalog: TOOLS,
    systemPrompt,
    omlx: { storagePrefix: 'myllm' }
  })
}
