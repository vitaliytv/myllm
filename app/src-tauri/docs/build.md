---
type: Rust Module
title: build.rs
resource: app/src-tauri/build.rs
docgen:
  crc: e20effdf
  model: omlx/gemma-4-e4b-it-OptiQ-4bit
  score: 100
  issues: judge:inaccurate:0.99
  judgeModel: openai-codex/gpt-5.4-mini
---

## Огляд

Збирає необхідні конфігураційні дані та залежності для коректного запуску застосунку. Ініціює процес збірки, використовуючи зібраний набір даних.

## Поведінка

1. Збирає конфігурацію та ресурси для збірки застосунку.
2. Ініціює процес збірки застосунку на основі конфігурації.

## Гарантії поведінки

- Read-only: не виконує операцій запису (ФС/БД).
