import type { TokensState } from '../../bindings/TokensState'
import type { TokenDay } from '../../bindings/TokenDay'

/** The Tokens block's state, the days with a reply, oldest first: Rust's types (`crates/blocks/src/tokens`), generated in `bindings/`. */
export type { TokensState, TokenDay }

/** The tools' names, as `[tokens] sources` writes them. */
export const TOOL_LABEL: Record<string, string> = { claude: 'Claude Code', codex: 'Codex', 'command-code': 'Command Code', opencode: 'OpenCode' }
