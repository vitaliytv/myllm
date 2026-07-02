<template>
  <q-layout view="hHh lpR fFf">
    <q-header elevated>
      <q-toolbar>
        <q-toolbar-title>
          myllm — omlx queue
          <span v-if="appVersion" class="text-caption text-blue-2">v{{ appVersion }}</span>
        </q-toolbar-title>
        <q-badge v-if="proxy.running.value" color="positive" class="q-mr-md">
          proxy :{{ proxy.port.value }}
        </q-badge>
        <q-btn @click="agentOpen = true" flat dense no-caps icon="sym_o_smart_toy" label="Агент" />
        <q-btn @click="auditOpen = true" flat dense round icon="sym_o_history" title="Журнал запитів агента" />
      </q-toolbar>
    </q-header>

    <AgentDialog v-model="agentOpen" :agent="agent" prompt-hint="наприклад: чому черга не рухається?" />
    <AuditDialog v-model="auditOpen" :agent="agent" />

    <q-page-container>
      <q-page class="q-pa-lg column q-gutter-md">
        <q-card flat bordered>
          <q-card-section>
            <div class="text-subtitle1 q-mb-sm">Підключення</div>
            <div class="row q-gutter-sm items-center">
              <q-input
                v-model="connection.baseUrl"
                dense
                outlined
                label="Upstream omlx URL"
                style="min-width: 260px"
                :disable="queue.connected.value" />
              <q-input
                v-model="connection.apiKey"
                dense
                outlined
                type="password"
                label="Admin API key"
                style="min-width: 220px"
                :disable="queue.connected.value" />
              <q-input
                v-model.number="connection.proxyPort"
                dense
                outlined
                type="number"
                label="Локальний порт проксі"
                style="width: 160px"
                :disable="queue.connected.value" />
              <q-btn
                v-if="!queue.connected.value"
                @click="connectAndStartProxy"
                color="primary"
                no-caps
                :loading="queue.connecting.value"
                label="Підключити" />
              <q-btn v-else @click="disconnectAll" outline no-caps color="negative" label="Відключити" />
            </div>
            <div v-if="queue.error.value" class="text-negative q-mt-sm">{{ queue.error.value }}</div>
            <div v-if="proxy.error.value" class="text-negative q-mt-sm">Проксі: {{ proxy.error.value }}</div>
            <div v-if="proxy.running.value" class="text-caption text-grey-7 q-mt-sm">
              Щоб бачити запити інших застосунків тут, направ їх на
              <code>http://127.0.0.1:{{ proxy.port.value }}/v1</code>
              замість <code>:8000/v1</code> і тримай myllm запущеним.
            </div>
          </q-card-section>
        </q-card>

        <template v-if="queue.connected.value">
          <div class="text-subtitle1">Черга зараз</div>
          <div class="row q-gutter-md">
            <q-card v-for="model in queue.snapshot.value.models" :key="model.id" flat bordered class="col-12 col-md-5">
              <q-card-section>
                <div class="text-subtitle2">{{ model.id }}</div>
                <div class="text-caption text-grey-7 q-mb-sm">
                  активні {{ model.active }}{{ queue.maxConcurrent.value ? ` / ${queue.maxConcurrent.value}` : '' }},
                  в черзі {{ model.waiting }}
                </div>
                <q-linear-progress
                  v-if="queue.maxConcurrent.value"
                  :value="model.active / queue.maxConcurrent.value"
                  color="primary"
                  class="q-mb-md" />

                <q-markup-table v-if="model.generatingList.length" dense flat>
                  <thead>
                    <tr>
                      <th class="text-left">Генерується</th>
                      <th class="text-right">токенів</th>
                      <th class="text-right">tok/s</th>
                      <th class="text-right">сек</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="g in model.generatingList" :key="g.requestId">
                      <td>{{ g.requestId.slice(0, 8) }}</td>
                      <td class="text-right">{{ g.generatedTokens }}</td>
                      <td class="text-right">{{ g.tokensPerSecond.toFixed(1) }}</td>
                      <td class="text-right">{{ g.elapsedSeconds.toFixed(1) }}</td>
                    </tr>
                  </tbody>
                </q-markup-table>

                <q-markup-table v-if="model.waitingList.length" dense flat class="q-mt-sm">
                  <thead>
                    <tr>
                      <th class="text-left">В черзі</th>
                      <th class="text-right">позиція</th>
                      <th class="text-right">prompt токенів</th>
                      <th class="text-right">сек</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="w in model.waitingList" :key="w.requestId">
                      <td>{{ w.requestId.slice(0, 8) }}</td>
                      <td class="text-right">{{ w.queuePosition }}</td>
                      <td class="text-right">{{ w.promptTokens }}</td>
                      <td class="text-right">{{ w.elapsedSeconds.toFixed(1) }}</td>
                    </tr>
                  </tbody>
                </q-markup-table>

                <div v-if="!model.active && !model.waiting" class="text-caption text-grey-6">
                  простій
                </div>
              </q-card-section>
            </q-card>
          </div>

          <div class="row items-center q-mt-md q-gutter-sm">
            <div class="text-subtitle1">Історія запитів</div>
            <q-btn
              v-if="history.entries.value.length"
              @click="clearHistory"
              flat
              dense
              no-caps
              color="negative"
              icon="sym_o_delete_sweep"
              label="Очистити" />
          </div>
          <q-card v-if="!history.entries.value.length" flat bordered>
            <q-card-section class="text-grey-7">
              Ще немає жодного запиту через проксі. Направ клієнта на
              <code>http://127.0.0.1:{{ proxy.port.value }}/v1</code> і зроби запит.
            </q-card-section>
          </q-card>
          <q-list v-else separator bordered class="rounded-borders">
            <q-item v-for="entry in history.entries.value" :key="entry.id" @click="toggleEntry(entry.id)" clickable>
              <q-item-section>
                <q-item-label>
                  <span class="text-weight-medium">{{ entry.path }}</span>
                  <span class="text-grey-6 q-ml-sm">{{ formatTime(entry.timestampMs) }}</span>
                  <q-badge v-if="entry.model" outline color="primary" class="q-ml-sm">{{ entry.model }}</q-badge>
                  <q-badge :color="entry.status < 400 ? 'positive' : 'negative'" class="q-ml-sm">
                    {{ entry.status }}
                  </q-badge>
                  <span class="text-caption text-grey-6 q-ml-sm">{{ entry.durationMs }}ms</span>
                </q-item-label>
                <q-item-label v-if="expandedEntries[entry.id]" caption class="entry-body">
                  <div class="text-weight-medium q-mt-sm">Запит</div>
                  <pre>{{ JSON.stringify(entry.requestBody, null, 2) }}</pre>
                  <div class="text-weight-medium q-mt-sm">Відповідь</div>
                  <pre>{{ entry.responseText }}</pre>
                </q-item-label>
              </q-item-section>
            </q-item>
          </q-list>
        </template>
      </q-page>
    </q-page-container>
  </q-layout>
</template>

<script setup>
import { getVersion } from '@tauri-apps/api/app'
import { invoke } from '@tauri-apps/api/core'
import { AgentDialog, AuditDialog } from '@7n/tauri-components/components'
import { Dialog } from 'quasar'
import { useAgent } from './composables/use-agent.js'
import { useOmlxQueue } from './composables/use-omlx-queue.js'
import { useProxy } from './composables/use-proxy.js'
import { useRequestHistory } from './composables/use-request-history.js'
import { useUpdater } from './composables/use-updater.js'
import { loadConnection, saveConnection } from './services/omlx-connection.js'

const agent = useAgent()
useUpdater()
const agentOpen = ref(false)
const auditOpen = ref(false)

const queue = useOmlxQueue()
const proxy = useProxy()
const history = useRequestHistory()

const connection = ref(loadConnection(localStorage))
const expandedEntries = ref({})
const appVersion = ref('')

/**
 * Логінить admin-сесію і піднімає локальний проксі.
 * @returns {Promise<void>}
 */
async function connectAndStartProxy() {
  saveConnection(localStorage, connection.value)
  await queue.connect(connection.value.baseUrl, connection.value.apiKey)
  if (queue.connected.value) {
    await proxy.start(connection.value.baseUrl, connection.value.proxyPort)
  }
}

// Автопідключення при старті: якщо ключа нема в localStorage — беремо
// OMLX_API_KEY з env процесу (є при запуску з shell; з Finder — ні),
// і якщо ключ є з будь-якого джерела, конектимось без кліку.
onMounted(async () => {
  appVersion.value = await getVersion()
  if (!connection.value.apiKey) {
    const envKey = await invoke('omlx_env_api_key')
    if (envKey) connection.value = { ...connection.value, apiKey: envKey }
  }
  if (connection.value.apiKey) await connectAndStartProxy()
})

/**
 * Зупиняє проксі та polling черги.
 * @returns {Promise<void>}
 */
async function disconnectAll() {
  await proxy.stop()
  queue.disconnect()
}

/**
 * Розгортає/згортає деталі запиту в списку історії.
 * @param {number} id `RequestLogEntry.id`
 * @returns {void}
 */
function toggleEntry(id) {
  expandedEntries.value = { ...expandedEntries.value, [id]: !expandedEntries.value[id] }
}

/**
 * @param {number} ms unix-час у мілісекундах
 * @returns {string} локальний час (лише час, без дати)
 */
function formatTime(ms) {
  return new Date(ms).toLocaleTimeString()
}

/** Очищає історію запитів після підтвердження користувачем. */
function clearHistory() {
  Dialog.create({
    title: 'Очистити історію запитів',
    message: 'Усі збережені запити та відповіді буде видалено безповоротно.',
    cancel: true,
    persistent: true,
  }).onOk(() => history.clear())
}
</script>

<style scoped>
.entry-body pre {
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 0.8rem;
  max-height: 320px;
  overflow-y: auto;
}
</style>
