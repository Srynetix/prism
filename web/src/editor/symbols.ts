import { ensureSyntaxTree, syntaxTree } from '@codemirror/language'
import type { EditorState } from '@codemirror/state'
import type { SyntaxNode } from '@lezer/common'

/**
 * A file's symbols, from its syntax tree (no language server): functions, classes, types and
 * Markdown headings, for quick open's `@` and the breadcrumb. What a grammar names them differs;
 * the legacy modes (TOML, shell…) have none.
 */
export interface Symbol {
  name: string
  kind: 'function' | 'class' | 'type' | 'heading'
  from: number
  /** Nested in a class, an impl, a heading. */
  depth: number
}

const KINDS: Record<string, Symbol['kind']> = {
  // JavaScript / TypeScript
  FunctionDeclaration: 'function', MethodDeclaration: 'function', ClassDeclaration: 'class',
  InterfaceDeclaration: 'type', TypeAliasDeclaration: 'type', EnumDeclaration: 'type',
  // Python
  FunctionDefinition: 'function', ClassDefinition: 'class',
  // Rust
  FunctionItem: 'function', StructItem: 'class', EnumItem: 'type', TraitItem: 'type', ImplItem: 'class', ModItem: 'class', TypeItem: 'type',
  // Markdown
  ATXHeading1: 'heading', ATXHeading2: 'heading', ATXHeading3: 'heading', ATXHeading4: 'heading', ATXHeading5: 'heading', ATXHeading6: 'heading',
  SetextHeading1: 'heading', SetextHeading2: 'heading',
}

/** The children that hold a declaration's name. */
const NAMES = new Set(['VariableDefinition', 'PropertyDefinition', 'TypeDefinition', 'VariableName', 'BoundIdentifier', 'TypeIdentifier', 'Identifier', 'PropertyName'])

function nameOf(state: EditorState, n: SyntaxNode): string | null {
  if (KINDS[n.name] === 'heading') return state.doc.sliceString(n.from, n.to).replace(/^#+\s*|\s*#*\s*$/g, '').split('\n')[0].trim() || null
  // `impl Trait for Type`: both, as written.
  if (n.name === 'ImplItem') {
    const body = n.getChild('DeclarationList')
    return state.doc.sliceString(n.from, body ? body.from : n.to).replace(/^impl\s*(<[^>]*>)?\s*/, '').replace(/\s+/g, ' ').trim() || null
  }
  for (let c = n.firstChild; c; c = c.nextSibling) {
    if (NAMES.has(c.name)) return state.doc.sliceString(c.from, c.to)
    // Python and Rust: the body is not looked into.
    if (c.name === 'Body' || c.name === 'Block' || c.name === 'ClassBody' || c.name === 'DeclarationList') break
  }
  return null
}

const headingLevel = (name: string) => Number(name.slice(-1)) || 1

/** The symbols of the file, in order; the tree parsed for at most `ms`. */
export function symbolsOf(state: EditorState, ms = 200): Symbol[] {
  const tree = ensureSyntaxTree(state, state.doc.length, ms) ?? syntaxTree(state)
  const out: Symbol[] = []
  const stack: number[] = []
  tree.iterate({
    enter(ref) {
      const kind = KINDS[ref.name]
      if (!kind) return
      const name = nameOf(state, ref.node)
      if (!name) return
      if (kind === 'heading') out.push({ name, kind, from: ref.from, depth: headingLevel(ref.name) - 1 })
      else {
        while (stack.length && stack[stack.length - 1] <= ref.from) stack.pop()
        out.push({ name, kind, from: ref.from, depth: stack.length })
        if (kind === 'class') stack.push(ref.to)
      }
    },
  })
  return out
}

/** The symbols holding `pos`, outermost first: the breadcrumb. */
export function symbolsAt(state: EditorState, pos: number): string[] {
  const out: string[] = []
  for (let n: SyntaxNode | null = syntaxTree(state).resolveInner(pos, -1); n; n = n.parent) {
    if (!KINDS[n.name] || KINDS[n.name] === 'heading') continue
    const name = nameOf(state, n)
    if (name) out.unshift(name)
  }
  if (out.length) return out
  // Markdown: the headings above.
  const heads = symbolsOf(state, 20).filter((s) => s.kind === 'heading' && s.from <= pos)
  const path: Symbol[] = []
  for (const h of heads) {
    while (path.length && path[path.length - 1].depth >= h.depth) path.pop()
    path.push(h)
  }
  return path.map((h) => h.name)
}
