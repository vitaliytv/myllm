---
docgen:
  source: app/src-tauri/src/omlx/mod.rs
  crc: 177989ad
  model: omlx/gemma-4-e4b-it-OptiQ-4bit
  score: 100
  issues: judge:inaccurate:0.98
  judgeModel: openai-codex/gpt-5.4-mini
---

# mod.rs

## Огляд

Файл надає механізм для імпорту та доступу до логіки окремих ланцюжків. Він є точкою входу для ініціалізації та взаємодії з механізмами виконання ланцюжків у системі.

## Поведінка

1. Імпортує модуль `chains` для доступу до логіки ланцюжків.

## Гарантії поведінки

- Read-only: не виконує операцій запису (ФС/БД).
