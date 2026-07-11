---
type: JS Module
title: stryker-vue-macros-ignorer.mjs
resource: app/stryker-vue-macros-ignorer.mjs
docgen:
  crc: 30a5e9f9
  model: omlx/gemma-4-e4b-it-OptiQ-4bit
  score: 100
  issues: judge:inaccurate:0.98
  judgeModel: openai-codex/gpt-5.4-mini
---

## Огляд

Цей плагін для Stryker ігнорує мутації, які генеруються викликами Vue `<script setup>`-макросів, таких як `defineProps`, `defineEmits`, `defineModel`, `defineSlots`, `defineExpose` та `defineOptions`. Це робиться для уникнення помилок компілятора `@vue/compiler-sfc`, оскільки ці макроси вимагають статичного аналізу на етапі `compile-sfc`. Плагін інтегрується в конфігурацію Stryker, додаючись у `plugins: ['./stryker-vue-macros-ignorer.mjs']` у `stryker.config.mjs` та активуючись через `ignorers: ['vue-macros']`. Механізм роботи контролюється публічними функціями `shouldIgnore` та `strykerPlugins`.

## Поведінка

Поведінка
shouldIgnore визначає, чи слід ігнорувати мутацію виклику, перевіряючи, чи викликається один із визначених макросів Vue `<script setup>` (`defineProps`, `defineEmits`, `defineModel`, `defineSlots`, `defineExpose`, `defineOptions`). Якщо це макрос, він повертає повідомлення про необхідність ігнорування; інакше повертає undefined.
strykerPlugins надає конфігурацію плагіна для Stryker, реєструючи об'єкт, що містить функцію `shouldIgnore` під іменем 'vue-macros'.

## Публічний API

Назва — Плагін для Stryker, який ігнорує мутації, пов'язані з викликами макросів Vue `<script setup>` (`defineProps`, `defineEmits`, `defineModel`, `defineSlots`, `defineExpose`, `defineOptions`). Це дозволяє уникнути помилок компіляції в `@vue/compiler-sfc`, оскільки ці макроси вимагають статичного аналізу на етапі `compile-sfc`. Для активації плагіна необхідно додати його у масив `plugins` у `stryker.config.mjs` та зареєструвати `vue-macros` у масиві `ignorers`. Плагін надає публічні функції `shouldIgnore` та `strykerPlugins`.

## Гарантії поведінки

- Read-only: не виконує операцій запису (ФС/БД).
