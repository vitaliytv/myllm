---
type: Rust Module
title: pi_agent.rs
resource: app/src-tauri/src/pi_agent.rs
docgen:
  crc: 3150bdda
  model: omlx/gemma-4-e4b-it-OptiQ-4bit
  tier: local-min
  score: 50
  issues: no-overview,short-behavior,anchor-miss:https://pi.dev,best-of-2:retry-lost
---

## Гарантії поведінки

- Read-only: не виконує операцій запису (ФС/БД).
- Перехоплює помилки і не пропускає винятків назовні (fail-safe).
- За певних помилок повертає порожнє значення (напр. `null`) замість винятку.
