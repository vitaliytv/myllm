<template>
  <q-layout view="hHh lpR fFf">
    <q-header elevated>
      <q-toolbar>
        <q-toolbar-title>
          myllm — LLM chains
          <span v-if="appVersion" class="text-caption text-blue-2">v{{ appVersion }}</span>
        </q-toolbar-title>
        <q-btn @click="agentOpen = true" flat dense no-caps icon="sym_o_smart_toy" label="Агент" />
        <q-btn @click="auditOpen = true" flat dense round icon="sym_o_history" title="Журнал запитів агента" />
      </q-toolbar>
    </q-header>

    <AgentDialog v-model="agentOpen" :agent="agent" prompt-hint="наприклад: чому черга не рухається?" />
    <AuditDialog v-model="auditOpen" :agent="agent" />
    <PiSessionDialog
      v-model="chainAnalysisOpen"
      @save="saveChainAnalysis"
      :entry="chainAnalysisEntry"
      :models="piAgent.models.value"
      :initial-prompt="chainAnalysisPrompt"
      saveable />

    <q-page-container>
      <q-page class="q-pa-lg column q-gutter-md">
        <ChainsPanel ref="chainsPanel" @analyze="openChainAnalysis" />
      </q-page>
    </q-page-container>
  </q-layout>
</template>

<script setup>
import { getVersion } from '@tauri-apps/api/app'
import { invoke } from '@tauri-apps/api/core'
import { AgentDialog, AuditDialog } from '@7n/tauri-components/components'
import { Notify } from 'quasar'
import ChainsPanel from './components/ChainsPanel.vue'
import PiSessionDialog from './components/PiSessionDialog.vue'
import { buildChainAnalysisPrompt, inferTargetRepo } from './services/chain-analysis.js'
import { useAgent } from './composables/use-agent.js'
import { usePiAgent } from './composables/use-pi-agent.js'
import { useUpdater } from '@7n/tauri-components/vue'

const agent = useAgent()
useUpdater()
const agentOpen = ref(false)
const auditOpen = ref(false)

const piAgent = usePiAgent()

const appVersion = ref('')
const chainsPanel = ref(null)
const chainAnalysisOpen = ref(false)
const chainAnalysisEntry = ref(null)
const chainAnalysisPrompt = ref('')
const chainAnalysisChain = ref(null)

onMounted(async () => {
  appVersion.value = await getVersion()
  await piAgent.loadModels()
})

/**
 * Відкриває pi-аналіз цілого ланцюжка: сесія стартує у cwd задачі, промпт —
 * таблиця кроків + завдання запропонувати зміни до інструмента-джерела.
 * @param {{chain: object, steps: Array<object>}} payload з ChainsPanel
 * @returns {void}
 */
function openChainAnalysis({ chain, steps }) {
  if (!chain.cwd || !piAgent.models.value.length) return
  chainAnalysisChain.value = chain
  chainAnalysisEntry.value = { id: `chain-${chain.chainId}`, client: { cwd: chain.cwd } }
  chainAnalysisPrompt.value = buildChainAnalysisPrompt({ chain, steps })
  chainAnalysisOpen.value = true
}

/**
 * Зберігає markdown-результат аналізу ланцюжка у ~/.n-cursor/insights/ і
 * оновлює бейджі вкладки «Ланцюжки».
 * @param {string} markdown остання відповідь агента
 * @returns {Promise<void>}
 */
async function saveChainAnalysis(markdown) {
  const chain = chainAnalysisChain.value
  if (!chain) return
  const path = await invoke('save_chain_analysis', {
    chainId: chain.chainId,
    chainKind: chain.chainKind,
    unit: chain.unit,
    cwd: chain.cwd,
    targetRepo: inferTargetRepo(chain.chainKind),
    model: chain.finalModel,
    markdown
  })
  Notify.create({ message: `Аналіз збережено: ${path}`, color: 'positive', timeout: 2500 })
  chainsPanel.value?.reload?.()
}
</script>
