/**
 * `@prism/sdk/words`: what a module's declarations (`defs.js`) may use, given before anything of
 * Prism reads the blocks. Its words in the user's language (`words` of `defs.js`, under Prism's).
 */
export type Vars = Record<string, string | number>
/** `text` in the user's language, its `{name}`s filled. */
export declare function t(text: string, vars?: Vars): string
/** `text` in a context (`tc('verb', 'Open')`). */
export declare function tc(context: string, text: string, vars?: Vars): string
/** A count's words: `tn(n, '{n} file', '{n} files')`. */
export declare function tn(n: number, one: string, other: string, vars?: Vars): string
