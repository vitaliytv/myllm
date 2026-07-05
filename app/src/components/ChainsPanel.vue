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
        Ланцюжків ще нема. Вони зʼявляються, коли клієнти @nitra/llm-lib (lint --fix, docgen, 7n-test)
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
                  <th class="text-right">час (проксі)</th>
                  <th class="text-left">помилка</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="s in joinedSteps[c.chainId] ?? []" :key="s.chainStep + (s.ts ?? '')">
                  <td>{{ s.chainStep }}</td>
                  <td>{{ s.kind }}</td>
                  <td>{{ s.model }}</td>
                  <td>{{ s.cloud ? 'cloud' : 'local' }}</td>
                  <td class="text-right">{{ s.usage?.totalTokens ?? '—' }}</td>
                  <td class="text-right">{{ s.request ? `${s.request.durationMs}ms` : '—' }}</td>
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
  </div>
</template>

<script setup>
import { chainAggregates, joinStepsWithRequests } from '../services/chains.js'
import { useChains } from '../composables/use-chains.js'

// Вкладка «Ланцюжки»: список задач із trace @nitra/llm-lib + аналітика.
// historyEntries — записи проксі для джойну локальних кроків (correlationId).
const props = defineProps({
  historyEntries: { type: Array, default: () => [] },
})
const emit = defineEmits(['analyze'])

const chains = useChains()
const expanded = ref({})
const joinedSteps = ref({})
const period = ref('week')

const PERIOD_MS = { day: 24 * 3600 * 1000, week: 7 * 24 * 3600 * 1000 }

const aggregates = computed(() =>
  chainAggregates(chains.chains.value, {
    sinceMs: period.value === 'all' ? undefined : Date.now() - PERIOD_MS[period.value],
  })
)

const visibleChains = computed(() => chains.chains.value.slice(0, 100))

/** Кольори outcome-бейджа. */
function outcomeColor(outcome) {
  if (outcome === 'success') return 'positive'
  if (outcome === 'partial') return 'warning'
  return 'negative'
}

/** Чи має ланцюжок збережений аналіз (індекс chain-analyses.jsonl). */
function hasAnalysis(chainId) {
  return chains.analyses.value.some(a => a?.chainId === chainId)
}

/** Розгортає ланцюжок; кроки вантажаться ліниво і джойняться з проксі-логом. */
async function toggle(c) {
  const open = !expanded.value[c.chainId]
  expanded.value = { ...expanded.value, [c.chainId]: open }
  if (open && !joinedSteps.value[c.chainId]) {
    const steps = await chains.loadSteps(c.chainId)
    joinedSteps.value = {
      ...joinedSteps.value,
      [c.chainId]: joinStepsWithRequests(steps, c.chainId, props.historyEntries),
    }
  }
}

/** Кнопка аналізу: віддає ланцюжок + кроки нагору (App відкриває pi-діалог). */
async function analyze(c) {
  if (!joinedSteps.value[c.chainId]) {
    const steps = await chains.loadSteps(c.chainId)
    joinedSteps.value = {
      ...joinedSteps.value,
      [c.chainId]: joinStepsWithRequests(steps, c.chainId, props.historyEntries),
    }
  }
  emit('analyze', { chain: c, steps: joinedSteps.value[c.chainId] })
}

/** Оновлює список і бейджі аналізів. */
async function reload() {
  await chains.load()
  await chains.loadAnalyses()
  expanded.value = {}
  joinedSteps.value = {}
}

defineExpose({ reload })
</script>
