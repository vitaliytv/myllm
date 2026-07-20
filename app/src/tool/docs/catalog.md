---
type: JS Module
title: catalog.js
resource: app/src/tool/catalog.js
docgen:
  crc: db957b1c
  model: openai-codex/gpt-5.6-luna
  tier: cloud-min
  score: 100
  issues: judge:inaccurate:0.98
  judgeModel: openai-codex/gpt-5.6-luna
---

## Огляд

Публічний API `TOOLS` визначає доступні інструменти каталогу. Каталог працює лише на читання та не записує дані у файлову систему чи базу даних.

## Поведінка

1. `TOOLS` містить порожній каталог доступних інструментів.
2. Каталог не додає доменних дій до звичайного чату через ACP.
3. Робота каталогу не змінює файли чи дані в базі.

## Гарантії поведінки

- Read-only: не виконує операцій запису (ФС/БД).
