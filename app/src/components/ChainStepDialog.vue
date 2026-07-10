<template>
  <BaseDialog
    @update:model-value="val => emit('update:modelValue', val)"
    :model-value="modelValue"
    :title="title"
    icon="sym_o_forum"
    body-class="step-dialog-body"
    :width="640">
    <template v-if="step">
      <div class="row items-center q-gutter-xs">
        <q-badge outline color="primary">{{ step.model ?? '—' }}</q-badge>
        <q-badge outline :color="isLocalModel(step.model) ? 'grey-7' : 'orange'">
          {{ isLocalModel(step.model) ? 'local' : 'cloud' }}
        </q-badge>
        <span class="text-caption text-grey-7">
          крок {{ step.chainStep }} · {{ step.usage?.totalTokens ?? '—' }} tok · {{ step.ts }}
        </span>
      </div>

      <q-banner v-if="step.error" dense class="bg-negative text-white q-mt-sm">{{ step.error }}</q-banner>

      <q-banner v-if="usingBody" dense class="bg-teal-1 text-teal-9 q-mt-sm">
        Повне (нестиснуте) тіло з body-capture стору.
      </q-banner>
      <q-banner v-else-if="!promptTurns.length && !responseText" dense class="bg-grey-3 text-grey-8 q-mt-sm">
        Промпт/відповідь відсутні у trace для цього кроку. Увімкніть
        <code>N_LLM_TRACE_BODIES=1</code> перед повторним запуском, щоб бачити повні тіла.
      </q-banner>

      <div v-if="promptTurns.length" class="text-caption text-weight-medium q-mt-md">Промпт</div>
      <div v-for="(m, i) in promptTurns" :key="i" class="turn">
        <div class="row items-center q-gutter-xs">
          <q-badge outline dense :color="roleColor(m.role)">{{ m.role }}</q-badge>
          <q-btn @click="copy(m.content)" flat dense round size="sm" icon="sym_o_content_copy" title="Копіювати" />
        </div>
        <pre class="turn-text">{{ m.content }}</pre>
      </div>

      <div v-if="responseText" class="text-caption text-weight-medium q-mt-md">Відповідь</div>
      <div v-if="responseText" class="turn">
        <div class="row items-center q-gutter-xs">
          <q-badge outline dense color="positive">assistant</q-badge>
          <q-btn @click="copy(responseText)" flat dense round size="sm" icon="sym_o_content_copy" title="Копіювати" />
        </div>
        <pre class="turn-text">{{ responseText }}</pre>
      </div>
    </template>

    <template #actions>
      <q-btn v-close-popup flat no-caps label="Закрити" />
    </template>
  </BaseDialog>
</template>

<script setup>
import { BaseDialog } from '@7n/tauri-components/components'
import { Notify, copyToClipboard } from 'quasar'
import { isLocalModel } from '../services/chains.js'

// Деталі одного кроку ланцюжка: повний промпт (messages) і відповідь моделі
// (content), з пріоритетом на нестиснуте body-capture тіло (`step.body`),
// якщо воно доступне — trace-запис сам може бути стиснутий клієнтом.
const props = defineProps({
  modelValue: { type: Boolean, default: false },
  step: { type: Object, default: null },
})
const emit = defineEmits(['update:modelValue'])

const usingBody = computed(() => Boolean(props.step?.body))

const title = computed(() => (props.step ? `Крок ${props.step.chainStep} · ${props.step.kind}` : 'Крок'))

/**
 * Нормалізує content повідомлення (string або масив parts) у текст.
 * @param {string|Array<object>|null|undefined} content сире поле content
 * @returns {string} текстове представлення
 */
function textOf(content) {
  if (typeof content === 'string') return content
  if (Array.isArray(content)) return content.filter(p => p?.type === 'text').map(p => p.text).join('')
  return content === null || content === undefined ? '' : String(content)
}

const promptTurns = computed(() => {
  if (usingBody.value) return [{ role: 'prompt', content: props.step.body.prompt ?? '' }]
  return (props.step?.messages ?? []).map(m => ({ role: m.role, content: textOf(m.content) }))
})

const responseText = computed(() => {
  if (usingBody.value) return props.step.body.output ?? ''
  return textOf(props.step?.content)
})

/**
 * Колір бейджа ролі повідомлення.
 * @param {string} role роль повідомлення (system/user/assistant/prompt)
 * @returns {string} назва кольору Quasar
 */
function roleColor(role) {
  if (role === 'system') return 'grey-7'
  if (role === 'user') return 'primary'
  return 'blue-grey'
}

/**
 * Копіює текст у буфер обміну з нотифікацією.
 * @param {string} text текст для копіювання
 * @returns {Promise<void>}
 */
async function copy(text) {
  try {
    await copyToClipboard(text ?? '')
    Notify.create({ message: 'Скопійовано', color: 'positive', timeout: 1200 })
  } catch (error) {
    Notify.create({ message: String(error?.message ?? error), color: 'negative', timeout: 2500 })
  }
}
</script>

<style scoped>
.step-dialog-body {
  max-height: 70vh;
  overflow-y: auto;
}

.turn {
  margin-top: 4px;
}

.turn-text {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-size: 12px;
  line-height: 1.45;
  font-family: 'SF Mono', ui-monospace, 'JetBrains Mono', monospace;
  background: color-mix(in srgb, currentColor 6%, transparent);
  border-radius: 6px;
  padding: 8px 10px;
  margin: 2px 0 8px;
}
</style>
