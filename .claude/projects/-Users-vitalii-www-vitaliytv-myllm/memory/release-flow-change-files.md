---
name: release-flow-change-files
description: Релізи в myllm і nitra/cursor робить CI з change-файлів — ніколи не правити version/CHANGELOG вручну
metadata:
  type: project
---

У репо `vitaliytv/myllm` і `nitra/cursor` (звідти ж `@7n/llm-lib`, workspace `llm-lib/`) релізи збирає CI на `main` (`changelog-release.yml` / `n-cursor release`): агрегує change-файли, бампить `version`, пише `CHANGELOG.md`, ставить тег і публікує (npm-publish).

**Why:** ручна правка `version`/`CHANGELOG.md` завалює `check changelog` (drift від реєстру).

**How to apply:** для кожного зміненого пакетного workspace класти `<ws>/.changes/YYMMDD-HHMM.md` із frontmatter `bump: major|minor|patch` + `section: Added|Changed|Fixed|Removed` і одним рядком опису (укр.). CLI-субкоманди `change` в поточній версії n-cursor нема — файл створювати вручну. Workspace-и: myllm → `app/`, cursor → `npm/`, `llm-lib/`. Кореневий `package.json` (bump `@nitra/cursor` у devDeps) change-файлу не потребує; його бампає pre-commit hook сам. Перевірка: `npx @nitra/cursor lint changelog` → exit 0. Пуш у `main` тригерить реліз одразу. Див. [[cursor-repo-lint-oxlint]].
