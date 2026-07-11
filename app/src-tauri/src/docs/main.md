---
type: Rust Module
title: main.rs
resource: app/src-tauri/src/main.rs
docgen:
  crc: 08907473
  model: omlx/gemma-4-e4b-it-OptiQ-4bit
  score: 100
  issues: judge:inaccurate:0.98
  judgeModel: openai-codex/gpt-5.4-mini
---

## Огляд

Роль цього файлу — ініціалізація та запуск основної логіки застосунку через виклик функції `myllm_lib::run`.

## Поведінка

1. Запускає основну логіку застосунку через функцію `myllm_lib::run`.

## Гарантії поведінки

- Read-only: не виконує операцій запису (ФС/БД).
