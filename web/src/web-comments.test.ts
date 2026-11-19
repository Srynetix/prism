import { describe, expect, it } from 'vitest'
import { annotateType, elementLabel, pinsOf, readFound, readPick, webPrompt } from './web-comments.svelte'

const pick = { prism: 'annotate', type: 'pick', sel: 'main > div.actions > button.cta', tag: 'button', text: ' Relancer   les jobs ', rect: { x: 10, y: 20, w: 120, h: 32 } }

describe('the annotation script’s messages', () => {
  it('are read only from Prism’s script', () => {
    expect(annotateType(pick)).toBe('pick')
    expect(annotateType({ type: 'pick' })).toBeNull()
    expect(annotateType('pick')).toBeNull()
    expect(annotateType(null)).toBeNull()
  })

  it('keep of a pick what a remark needs, bounded', () => {
    expect(readPick(pick)).toEqual({ sel: 'main > div.actions > button.cta', tag: 'button', text: 'Relancer les jobs', rect: { x: 10, y: 20, w: 120, h: 32 } })
    const long = readPick({ ...pick, sel: 'x'.repeat(1000), text: 'y'.repeat(500), extra: '<script>' })!
    expect([long.sel.length, long.text.length, 'extra' in long]).toEqual([300, 80, false])
    expect(readPick({ ...pick, rect: { x: 1, y: 'a', w: 1, h: 1 } }), 'a rectangle of numbers').toBeNull()
    expect(readPick({ ...pick, tag: 'img onerror=x' }), 'a tag name').toBeNull()
    expect(readPick({ ...pick, sel: 42 })).toBeNull()
    expect(readPick({ ...pick, type: 'found' })).toBeNull()
  })

  it('say which remarks the page found again', () => {
    const found = readFound({ prism: 'annotate', type: 'found', pins: [{ n: 1, missing: false }, { n: 2, missing: true }, { n: 'x' }, null] })
    expect([...found!]).toEqual([1])
    expect(readFound({ prism: 'annotate', type: 'found', pins: 'all' })).toBeNull()
  })
})

describe('the remarks', () => {
  const notes = [
    { id: 'a', sel: 'header.hero > h1', tag: 'h1', text: 'Pipeline #4821', note: 'Link the pipeline.' },
    { id: 'b', sel: 'main > button.cta', tag: 'button', text: '', note: 'Too loud:\nmake it secondary.' },
  ]

  it('are pinned on the page from 1, the one pointed at standing out', () => {
    expect(pinsOf(notes, 2)).toEqual([
      { n: 1, sel: 'header.hero > h1', tag: 'h1', text: 'Pipeline #4821', hl: false },
      { n: 2, sel: 'main > button.cta', tag: 'button', text: '', hl: true },
    ])
    expect(elementLabel(notes[0])).toBe('<h1> “Pipeline #4821”')
    expect(elementLabel(notes[1])).toBe('<button>')
  })

  it('go as one prompt: the page, the whole page’s remark, then each element with its picture', () => {
    const prompt = webPrompt('@scratchpad/report.html', { whole: ' Too busy. ', notes }, { 2: '.prism/attach/element 2.png' })
    expect(prompt).toBe(
      [
        'Remarks on the page report.html (the artifact in your session\'s scratchpad) (3 remarks, made on its elements in Prism\'s Web view). Address each, then say what you changed.',
        '',
        '- The whole page: Too busy.',
        '',
        '1. <h1> “Pipeline #4821” — header.hero > h1',
        '   Link the pipeline.',
        '',
        '2. <button> — main > button.cta',
        '   Too loud:',
        '   make it secondary.',
        '   @".prism/attach/element 2.png"',
      ].join('\n'),
    )
    expect(webPrompt('docs/a.html', { whole: '', notes: [notes[0]] })).toMatch(/^Remarks on the page docs\/a\.html \(1 remark,/)
    expect(webPrompt('@scratchpad/r.html', { whole: '', notes: [notes[0]] }, {}, '/tmp/s/r.html')).toMatch(/^Remarks on the page \/tmp\/s\/r\.html \(1 remark,/)
  })
})
