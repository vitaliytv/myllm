import { CODEX_ACP_AGENT_PRESET } from '@7n/tauri-components'
import { useAcpAgent } from '@7n/tauri-components/vue'
import { homeDir } from '@tauri-apps/api/path'
import { TOOLS } from '../tool/catalog.js'

// In-app ACP agent gateway for myllm — replaces the removed omlx/runAgent
// useAgent() (see CHANGELOG @7n/tauri-components@0.11.0). Domain catalog +
// spawn presets stay here; systemPrompt is gone — ACP agents read AGENTS.md /
// CLAUDE.md from cwd (see @7n/tauri-components SPEC §3.2–3.3).
//
// cwd is homeDir() as a sane default for spawned CLIs (packaged app has no
// checkout of this repo). Falls back to "." outside Tauri so a missing home
// dir can't crash the module graph via an unhandled top-level await rejection.
let cwd
try {
  cwd = await homeDir()
} catch {
  cwd = '.'
}

/**
 * @returns {object} the in-app ACP agent gateway (agentKind/modelTier refs, journal, loadEnv/request/respond/approve)
 */
export function useAgent() {
  return useAcpAgent({
    catalog: TOOLS,
    cwd,
    agents: {
      codex: CODEX_ACP_AGENT_PRESET,
      cursor: {
        command: 'cursor',
        args: ['agent', 'acp'],
        tiers: {
          MIN: { label: 'GPT-5 Mini', args: ['--model', 'gpt-5-mini'] },
          AVG: { label: 'Grok 4.5', args: ['--model', 'cursor-grok-4.5-high'] },
          MAX: { label: 'Auto', args: ['--model', 'auto'] }
        }
      },
      pi: {
        // pi-acp hardcodes its own spawn args and has no model passthrough —
        // model comes from ~/.pi/agent/settings.json.
        command: 'npx',
        args: ['-y', 'pi-acp']
      }
    }
  })
}
