<template>
  <div class="column q-gutter-md">
    <q-card flat bordered>
      <q-card-section>
        <div class="row items-center q-gutter-sm">
          <div class="text-subtitle1">Аналітика ланцюжків</div>
          <q-btn-toggle
            v-model="period"
            :options="[
              { label: '24h', value: 'day' },
              { label: '7d', value: 'week' },
              { label: 'все', value: 'all' },
            ]"
            dense
            no-caps
            unelevated
            toggle-color="primary" />
          <q-space />
          <q-btn @click="reload" flat dense round icon="sym_o_refresh" title="Оновити з trace" />
        </div>
        <div class="text-caption text-grey-7 q-mt-xs">
          Разом: {{ aggregates.totals.chains }} ланцюжків · cloud-викликів {{ aggregates.totals.cloudCalls }} ·
          cloud-токенів {{ aggregates.totals.cloudTokens }}
        </div>

        <q-markup-table v-if="aggregates.perKind.length" dense flat class="q-mt-sm">
          <thead>
            <tr>
              <th class="text-left">kind</th>
              <th class="text-right">chains</th>
              <th class="text-right">ok/part/fail</th>
              <th class="text-right">ескалація %</th>
              <th class="text-right">cloud calls</th>
              <th class="text-right">cloud tokens</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="k in aggregates.perKind" :key="k.kind">
              <td>{{ k.kind }}</td>
              <td class="text-right">{{ k.chains }}</td>
              <td class="text-right">{{ k.success }}/{{ k.partial }}/{{ k.fail }}</td>
              <td class="text-right">{{ Math.round(k.escalationRate * 100) }}</td>
              <td class="text-right">{{ k.cloudCalls }}</td>
              <td class="text-right">{{ k.cloudTokens }}</td>
            </tr>
          </tbody>
        </q-markup-table>

        <template v-if="aggregates.alwaysEscalatedUnits.length">
          <div class="text-caption text-weight-medium q-mt-sm">
            Кандидати на T0-скрипти (завжди ескалюють у cloud):
          </div>
          <div class="q-gutter-xs q-mt-xs">
            <q-badge v-for="u in aggregates.alwaysEscalatedUnits.slice(0, 10)" :key="u.kind + u.unit" outline color="orange">
              [{{ u.kind }}] {{ u.unit }} · {{ u.cloudTokens }} tok
            </q-badge>
          </div>
        </template>
      </q-card-section>
    </q-card>

    <q-card v-if="!chains.chains.value.length" flat bordered>
      <q-card-section class="text-grey-7">
        Ланцюжків ще нема. Вони зʼявляються, коли клієнти @7n/llm-lib (lint --fix, docgen, 7n-test)
        пишуть у <code>~/.n-cursor/llm-trace.jsonl</code>.
        <span v-if="chains.error.value" class="text-negative">{{ chains.error.value }}</span>
      </q-card-section>
    </q-card>

    <q-list v-else separator bordered class="rounded-borders">
      <q-item v-for="c in visibleChains" :key="c.chainId" @click="toggle(c)" clickable>
        <q-item-section>
          <q-item-label>
            <q-badge outline color="primary">{{ c.chainKind }}</q-badge>
            <span class="text-weight-medium q-ml-sm">{{ c.unit }}</span>
            <q-badge :color="outcomeColor(c.outcome)" class="q-ml-sm">{{ c.outcome }}</q-badge>
            <q-badge v-if="c.escalated" color="orange" outline class="q-ml-sm">local→cloud</q-badge>
            <q-badge v-if="hasAnalysis(c.chainId)" color="teal" outline class="q-ml-sm">має аналіз</q-badge>
            <span class="text-caption text-grey-6 q-ml-sm">
              кроків {{ c.steps }} · local {{ c.localCalls }} / cloud {{ c.cloudCalls }} ·
              {{ Math.round(c.wallMs / 1000) }}s
            </span>
          </q-item-label>
          <q-item-label caption>
            {{ c.ts }} <span v-if="c.cwd" class="q-ml-sm">{{ c.cwd }}</span>
          </q-item-label>

          <q-item-label v-if="expanded[c.chainId]" caption>
            <q-markup-table dense flat class="q-mt-sm">
              <thead>
                <tr>
                  <th class="text-left">#</th>
                  <th class="text-left">kind</th>
                  <th class="text-left">model</th>
                  <th class="text-left">де</th>
                  <th class="text-right">tokens</th>
                  <th class="text-left">помилка</th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="s in loadedSteps[c.chainId] ?? []"
                  :key="s.chainStep + (s.ts ?? '')"
                  @click.stop="openStep(c, s)"
                  class="step-row">
                  <td>{{ s.chainStep }}</td>
                  <td>{{ s.kind }}</td>
                  <td>{{ s.model }}</td>
                  <td>{{ isLocalModel(s.model) ? 'local' : 'cloud' }}</td>
                  <td class="text-right">{{ s.usage?.totalTokens ?? '—' }}</td>
                  <td class="ellipsis" style="max-width: 260px">{{ s.error ?? '' }}</td>
                </tr>
              </tbody>
            </q-markup-table>
          </q-item-label>
        </q-item-section>
        <q-item-section side top>
          <q-btn
            @click.stop="analyze(c)"
            :disable="!c.cwd"
            flat
            dense
            round
            size="sm"
            icon="sym_o_smart_toy"
            :title="c.cwd ? 'Аналіз ланцюжка через pi у cwd задачі' : 'cwd невідомий — аналіз недоступний'" />
        </q-item-section>
      </q-item>
    </q-list>

    <ChainStepDialog v-model="stepDialogOpen" :step="selectedStep" />
  </div>
</template>

<script setup>
import { chainAggregates, isLocalModel, joinStepsWithBodies } from '../services/chains.js'
import { useChains } from '../composables/use-chains.js'
import ChainStepDialog from './ChainStepDialog.vue'

// Вкладка «Ланцюжки»: список задач із trace @7n/llm-lib + аналітика.
const emit = defineEmits(['analyze'])

const chains = useChains()
const expanded = ref({})
const loadedSteps = ref({})
const period = ref('week')
const stepDialogOpen = ref(false)
const selectedStep = ref(null)

const PERIOD_MS = { day: 24 * 3600 * 1000, week: 7 * 24 * 3600 * 1000 }

const aggregates = computed(() =>
  chainAggregates(chains.chains.value, {
    sinceMs: period.value === 'all' ? undefined : Date.now() - PERIOD_MS[period.value],
  })
)

const visibleChains = computed(() => chains.chains.value.slice(0, 100))

/**
 * Кольори outcome-бейджа.
 * @param {string} outcome success|partial|fail
 * @returns {string} назва кольору Quasar
 */
function outcomeColor(outcome) {
  if (outcome === 'success') return 'positive'
  if (outcome === 'partial') return 'warning'
  return 'negative'
}

/**
 * Чи має ланцюжок збережений аналіз (індекс chain-analyses.jsonl).
 * @param {string} chainId id ланцюжка
 * @returns {boolean} true — аналіз збережено
 */
function hasAnalysis(chainId) {
  return chains.analyses.value.some(a => a?.chainId === chainId)
}

/**
 * Розгортає ланцюжок; кроки вантажаться ліниво і кешуються.
 * @param {object} c нормалізований ланцюжок зі списку
 * @returns {Promise<void>}
 */
async function toggle(c) {
  const open = !expanded.value[c.chainId]
  expanded.value = { ...expanded.value, [c.chainId]: open }
  if (open && !loadedSteps.value[c.chainId]) {
    loadedSteps.value = { ...loadedSteps.value, [c.chainId]: await chains.loadSteps(c.chainId) }
  }
}

/**
 * Відкриває деталі кроку (промпт/відповідь) — збагачує повним тілом з
 * opt-in body-capture стору, якщо воно є для цього ланцюжка (не помилка,
 * якщо `N_LLM_TRACE_BODIES` не вмикали — лишається trace-версія кроку).
 * @param {object} c нормалізований ланцюжок зі списку
 * @param {object} s нормалізований крок (parseChainStep)
 * @returns {Promise<void>}
 */
async function openStep(c, s) {
  const bodies = await chains.loadBodies(c.chainId)
  const [enriched] = joinStepsWithBodies([s], bodies)
  selectedStep.value = enriched
  stepDialogOpen.value = true
}

/**
 * Кнопка аналізу: віддає ланцюжок + кроки нагору (App відкриває pi-діалог).
 * Кроки збагачуються повними тілами з opt-in body-capture стору (працює й
 * для cloud-кроків) — якщо `N_LLM_TRACE_BODIES` не вмикали, `loadBodies`
 * повертає порожній список і кроки лишаються без `body`.
 * @param {object} c нормалізований ланцюжок зі списку
 * @returns {Promise<void>}
 */
async function analyze(c) {
  if (!loadedSteps.value[c.chainId]) {
    loadedSteps.value = { ...loadedSteps.value, [c.chainId]: await chains.loadSteps(c.chainId) }
  }
  const bodies = await chains.loadBodies(c.chainId)
  const enrichedSteps = joinStepsWithBodies(loadedSteps.value[c.chainId], bodies)
  emit('analyze', { chain: c, steps: enrichedSteps })
}

/** Оновлює список і бейджі аналізів. */
async function reload() {
  await chains.load()
  await chains.loadAnalyses()
  expanded.value = {}
  loadedSteps.value = {}
}

defineExpose({ reload })
</script>

<style scoped>
.step-row {
  cursor: pointer;
}

.step-row:hover {
  background: color-mix(in srgb, currentColor 6%, transparent);
}
</style>
