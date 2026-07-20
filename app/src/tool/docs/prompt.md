---
type: JS Module
title: prompt.js
resource: app/src/tool/prompt.js
docgen:
  crc: eadcc2ca
  model: openai-codex/gpt-5.6-luna
  tier: cloud-min
  score: 100
  issues: judge:inaccurate:0.99
  judgeModel: openai-codex/gpt-5.6-luna
---

## Огляд

Публічна функція `systemPrompt` надає системний опис. Код працює в режимі read-only щодо файлової системи та бази даних.

## Поведінка

1. `systemPrompt` визначає роль асистента як локального монітора черги omlx у застосунку myllm.
2. `systemPrompt` повідомляє, що взаємодія відбувається лише у форматі звичайної розмови через локальну модель.
3. `systemPrompt` не надає інструментів для виконання дій.
4. Використання `systemPrompt` не змінює файли чи дані.

## Гарантії поведінки

- Read-only: не виконує операцій запису (ФС/БД).
