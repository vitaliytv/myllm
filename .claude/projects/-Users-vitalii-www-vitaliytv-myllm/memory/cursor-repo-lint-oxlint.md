---
name: cursor-repo-lint-oxlint
description: CI Lint JS у myllm і nitra/cursor ганяє oxlint (суворіший за локальний eslint) — перевіряти bunx oxlint перед пушем
metadata:
  type: project
---

CI «Lint JS» в обох репо (`vitaliytv/myllm`, `nitra/cursor`) запускає `bunx oxlint` (+ eslint, jscpd, knip у myllm), і oxlint ловить правила, які локальний `bunx eslint` пропускає: `no-empty-function` (порожні стрілки → додати тіло-коментар `/* no-op … */`), `unicorn/consistent-function-scoping` (функції без замикань виносити на module scope), `unicorn/no-useless-undefined` (не передавати `undefined` явно).

**Why:** локально чистий eslint двічі не впіймав падіння CI (2026-07-10).

**How to apply:** перед пушем прогнати `bunx oxlint <змінені файли>` окремою командою і дивитись exit-код (не ховати за `| tail`). Пов'язане: [[release-flow-change-files]].
