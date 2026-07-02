---
name: docs-regen
description: Регенерувати C4-документацію MLMaiL у docs/ci4/ з clean ADR у docs/adr/ через LLM
---

# /docs-regen — регенерація C4-документації MLMaiL

Регенерує 5 файлів у `docs/ci4/` (`01-context.md`, `02-containers.md`, `03-components.md`, `04-code.md`, `decisions.md`) з clean ADR у `docs/adr/`. Для кожного clean ADR (без `session:` у frontmatter) — додає sentinel-блок `**Опрацьовано**` з посиланнями на проекції.

## Коли запускати

- Після того, як `/n-adr-normalize` перетворив накопичені drafts на clean ADR.
- Після ручного додавання / редагування ADR у `docs/adr/`.
- Коли треба синхронізувати C4-документацію з ADR перед PR.

## Запуск

`bun run docs:regen`

Опції:

- `bun run docs:regen --projection 01-context` — лише одна проекція.
- `bun run docs:regen --all` — форсити regen усіх, ігноруючи мітки.
- `bun run docs:regen --dry` — план без LLM-викликів.
- `bun run docs:regen --check` — CI-режим, fail на drift.

## Перевірка після запуску

`git status && git diff docs/ci4/ docs/adr/` — review-вікно. Розробник вирішує commit / rollback.

## Деталі

- Spec: `docs/superpowers/specs/2026-05-18-docs-regen-design.md`.
- Implementation: `scripts/docs-regen.js`.
- Шаблони промптів: `docs/ci4/_templates/*.prompt.md`.
- Tracking: `docs/ci4/manifest.json`.
