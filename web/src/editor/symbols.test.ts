import { describe, expect, it } from 'vitest'
import { EditorState, type Extension } from '@codemirror/state'
import { javascript } from '@codemirror/lang-javascript'
import { python } from '@codemirror/lang-python'
import { rust } from '@codemirror/lang-rust'
import { markdown } from '@codemirror/lang-markdown'
import { symbolsAt, symbolsOf } from './symbols'

const of = (doc: string, lang: Extension) => EditorState.create({ doc, extensions: lang })
const names = (s: EditorState) => symbolsOf(s).map((x) => `${'  '.repeat(x.depth)}${x.kind[0]} ${x.name}`)

describe('symbols', () => {
  it('reads TypeScript declarations, methods nested in their class', () => {
    const s = of('export function total(a: number) { return a }\nclass Cart {\n  add(x) {}\n  get size() { return 1 }\n}\ninterface Line { q: number }\ntype Id = string\n', javascript({ typescript: true }))
    expect(names(s)).toEqual(['f total', 'c Cart', '  f add', '  f size', 't Line', 't Id'])
    expect(symbolsAt(s, s.doc.toString().indexOf('add(x)') + 8)).toEqual(['Cart', 'add'])
  })
  it('reads Python and Rust', () => {
    expect(names(of('class A:\n    def f(self):\n        pass\ndef g():\n    pass\n', python()))).toEqual(['c A', '  f f', 'f g'])
    expect(names(of('struct S { a: u8 }\nimpl S {\n    fn new() -> Self { S { a: 0 } }\n}\nfn main() {}\n', rust()))).toEqual(['c S', 'c S', '  f new', 'f main'])
  })
  it('reads Markdown headings by level', () => {
    const s = of('# Title\n\ntext\n\n## Part\n\nmore\n', markdown())
    expect(names(s)).toEqual(['h Title', '  h Part'])
    expect(symbolsAt(s, s.doc.length - 2)).toEqual(['Title', 'Part'])
  })
})
