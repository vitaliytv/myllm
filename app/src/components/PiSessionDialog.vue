<template>
  <BaseDialog
    @update:model-value="val => emit('update:modelValue', val)"
    @show="onShow"
    :model-value="modelValue"
    :title="selectedModel ? `pi — ${selectedModel.label}` : 'pi'"
    icon="sym_o_terminal"
    :width="560">
    <div class="pi-session-scroll q-gutter-sm">
      <div class="text-caption text-grey-7">
        <q-icon name="sym_o_folder" size="14px" class="q-mr-xs" />{{ entry?.client?.cwd }}
      </div>

      <q-select
        v-model="selectedModel"
        :options="models"
        option-label="label"
        dense
        outlined
        label="Модель"
        hint="з env-змінних N_*_MODEL">
        <template #option="scope">
          <q-item v-bind="scope.itemProps">
            <q-item-section>
              <q-item-label>{{ scope.opt.label }}</q-item-label>
              <q-item-label caption>{{ scope.opt.model }}</q-item-label>
            </q-item-section>
          </q-item>
        </template>
      </q-select>

      <div v-if="turns.length" ref="logEl" class="chat-log">
        <template v-for="(turn, i) in turns" :key="i">
          <div v-if="turn.role === 'user'" class="chat-user">{{ turn.text }}</div>
          <pre v-else class="chat-agent" :class="{ 'text-negative': turn.isError }">{{ turn.text }}</pre>
        </template>
        <div v-if="running" class="chat-thinking"><q-spinner-dots size="18px" /> pi думає…</div>
      </div>

      <q-input
        v-model="prompt"
        @keyup.ctrl.enter="send"
        dense
        outlined
        autofocus
        type="textarea"
        autogrow
        :input-style="{ maxHeight: '30vh', overflowY: 'auto' }"
        :label="inputLabel"
        hint="Ctrl+Enter — надіслати" />
    </div>

    <template #actions>
      <q-btn
        v-if="saveable && lastAgentText"
        @click="emit('save', lastAgentText)"
        flat
        no-caps
        color="teal"
        icon="sym_o_save"
        label="Зберегти аналіз" />
      <DialogActions
        @submit="send"
        cancel-label="Закрити"
        :submit-label="sendLabel"
        icon="sym_o_play_arrow"
        :disable="sendDisabled"
        :loading="running" />
    </template>
  </BaseDialog>
</template>

<script setup>
import { BaseDialog, DialogActions } from '@7n/tauri-components/components'
import { usePiAgent } from '../composables/use-pi-agent.js'
import { buildAnalysisPrompt } from '../services/pi-prompt.js'

// Чат-сесія із зовнішнім кодинг-агентом `pi`, запущеним у cwd клієнта проксі-
// запиту. Кожна репліка — окремий короткоживучий виклик `pi --session-id …`;
// безперервність розмови тримає сам `pi` через сесійний файл на диску, тож тут
// не треба довготривалого child-процесу — лише той самий `sessionId` щоразу.
const props = defineProps({
  modelValue: { type: Boolean, default: false },
  entry: { type: Object, default: null },
  models: { type: Array, default: () => [] }, // [{key, label, model}] з N_*_MODEL env
  // Готовий стартовий промпт (chain-аналіз) — перекриває buildAnalysisPrompt(entry).
  initialPrompt: { type: String, default: '' },
  // Показує кнопку «Зберегти аналіз» (емітить 'save' з останньою відповіддю агента).
  saveable: { type: Boolean, default: false }
})
const emit = defineEmits(['update:modelValue', 'save'])

const piAgent = usePiAgent()

const prompt = ref('')
const running = ref(false)
const turns = ref([])
const logEl = ref(null)
const selectedModel = ref(null)

const inputLabel = computed(() => (turns.value.length ? 'Повідомлення' : 'Prompt'))
const sendLabel = computed(() => (turns.value.length ? 'Надіслати' : 'Запустити'))
const sendDisabled = computed(() => running.value || !prompt.value.trim() || !selectedModel.value)
const lastAgentText = computed(() => turns.value.findLast(t => t.role === 'agent' && !t.isError)?.text ?? '')

// Сесія прив'язана лише до запису історії — модель можна міняти між ходами
// одного й того ж діалогу, `pi` продовжує той самий контекст незалежно від того,
// яка модель відповідає на конкретний хід.
const sessionId = computed(() => `myllm-${props.entry?.id}`)

/** Скидає чат, обирає першу модель зі списку і заповнює перше повідомлення. */
function onShow() {
  turns.value = []
  running.value = false
  selectedModel.value = props.models[0] ?? null
  prompt.value = props.initialPrompt || (props.entry ? buildAnalysisPrompt(props.entry) : '')
}

/** Прокручує лог до останньої репліки після оновлення DOM. */
async function scrollToEnd() {
  await nextTick()
  if (logEl.value) logEl.value.scrollTop = logEl.value.scrollHeight
}

/** Надсилає поточний текст як хід тієї самої pi-сесії. */
async function send() {
  const text = prompt.value.trim()
  if (!text || running.value || !props.entry?.client?.cwd || !selectedModel.value) return
  prompt.value = ''
  turns.value.push({ role: 'user', text })
  scrollToEnd()
  running.value = true
  try {
    const output = await piAgent.send({
      cwd: props.entry.client.cwd,
      model: selectedModel.value.model,
      sessionId: sessionId.value,
      prompt: text
    })
    turns.value.push({ role: 'agent', text: output })
  } catch (error) {
    turns.value.push({ role: 'agent', text: String(error?.message ?? error), isError: true })
  } finally {
    running.value = false
    scrollToEnd()
  }
}
</script>

<style scoped>
/* BaseDialog's body q-card-section has no height cap on its own — with a
   long prompt/response this pushes the footer (send button) off-screen.
   Scrolling this inner wrapper instead keeps header/footer always visible. */
.pi-session-scroll {
  max-height: 60vh;
  overflow-y: auto;
  padding-right: 2px;
}

.chat-log {
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-height: 46vh;
  overflow-y: auto;
  padding: 2px;
}

.chat-user {
  align-self: flex-end;
  max-width: 85%;
  padding: 7px 11px;
  border-radius: 12px 12px 2px;
  background: color-mix(in srgb, #0a84ff 18%, transparent);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-size: 13px;
  line-height: 1.45;
}

.chat-agent {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-size: 13px;
  line-height: 1.45;
  font-family: inherit;
  margin: 0;
}

.chat-thinking {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  opacity: 0.7;
}
</style>
