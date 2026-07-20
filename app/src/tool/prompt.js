// Domain system prompt for the myllm agent (queue-monitor chat).
//
// TODO(@7n/tauri-components@0.11.0 CHANGELOG / SPEC §3.2): useAcpAgent no longer
// accepts systemPrompt — ACP agents read AGENTS.md/CLAUDE.md from cwd. Move this
// guidance into the repo AGENTS.md (or a cwd the packaged app can point at) and
// drop this unused export once that lands.

export const systemPrompt =
  'Ти асистент у застосунку myllm — локальному моніторі черги omlx. ' +
  'Інструментів немає, лише звичайна розмова через локальну модель.'
