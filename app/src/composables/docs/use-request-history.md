---
docgen:
  source: app/src/composables/use-request-history.js
  crc: 1c383a1c
  model: omlx/gemma-4-e4b-it-OptiQ-4bit
  score: 100
  issues: judge:inaccurate:0.98
  judgeModel: openai-codex/gpt-5.4-mini
---

# use-request-history.js

## Огляд

Модуль агрегує історію запитів, завантажуючи її з файлу `requests.jsonl` при ініціалізації. Він оновлює цей список, додаючи нові записи, що надходять через подію `omlx-request-logged`. Публічна функція `useRequestHistory` надає доступ до цієї історії, але не змінює файлову систему чи базу даних. Модуль також надає механізми для повного перезавантаження або очищення історії з файлу та пам'яті.

## Поведінка

1. `useRequestHistory` ініціалізує список записів історії.
2. При монтуванні компонента `useRequestHistory` завантажує історію із файлу `requests.jsonl`, встановлюючи її у список записів.
3. При монтуванні компонента `useRequestHistory` підписується на подію `omlx-request-logged`.
4. При отриманні події `omlx-request-logged` `useRequestHistory` додає новий запис на початок списку записів, обмежуючи загальну кількість записів у пам'яті.
5. `useRequestHistory` надає функцію `load`, яка перезаповнює список записів із `requests.jsonl`.
6. `useRequestHistory` надає функцію `clear`, яка видаляє історію з файлу `requests.jsonl` та очищає список записів у пам'яті.
7. При демонтажі компонента `useRequestHistory` відписується від події `omlx-request-logged`.

## Публічний API

useRequestHistory — Зберігає історію запитів до `/v1/*`, заповнюючи її з файлу `requests.jsonl` та оновлюючи в реальному часі через подію `omlx-request-logged`.

## Гарантії поведінки

- Read-only: не виконує операцій запису (ФС/БД).
