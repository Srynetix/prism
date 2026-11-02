import { anchored } from './popup'

/**
 * Suggestions under a text field, drawn by the app: a `<datalist>`'s list is the system's (Aqua on
 * macOS, GTK on Linux, each with its own look and keys), and its arrow too. The ones that contain
 * what is typed (all of them on an empty field) show on focus and as it is typed; ↑ ↓ move,
 * Enter or a click picks, Esc or leaving the field closes. A pick writes the field and sends it
 * `input` then `change`, as a datalist's did, so `bind:value` and `onchange` see it.
 */
export type Suggestion = string | { value: string; label?: string }

/** Beyond, the list stops (typing narrows it). */
const SHOWN = 80

export function suggest(options: () => Suggestion[]) {
  return (input: HTMLInputElement) => {
    let list: HTMLDivElement | null = null
    let unplace: (() => void) | undefined
    let shown: { value: string; label?: string }[] = []
    let cursor = -1
    let picking = false

    const close = () => {
      unplace?.()
      unplace = undefined
      list?.remove()
      list = null
      cursor = -1
      input.removeAttribute('aria-expanded')
    }
    const mark = () => {
      list?.querySelectorAll('.suggest-item').forEach((el, i) => el.classList.toggle('on', i === cursor))
      list?.querySelector('.suggest-item.on')?.scrollIntoView({ block: 'nearest' })
    }
    const pick = (value: string) => {
      input.value = value
      close()
      picking = true
      input.dispatchEvent(new Event('input', { bubbles: true }))
      input.dispatchEvent(new Event('change', { bubbles: true }))
      picking = false
    }
    const open = () => {
      const typed = input.value.trim().toLowerCase()
      shown = options()
        .map((o) => (typeof o === 'string' ? { value: o } : o))
        .filter((o) => !typed || o.value.toLowerCase().includes(typed) || o.label?.toLowerCase().includes(typed))
        // What is typed in full is no suggestion.
        .filter((o) => o.value.toLowerCase() !== typed)
        .slice(0, SHOWN)
      if (!shown.length || input.disabled || input.readOnly) return close()
      if (!list) {
        list = document.createElement('div')
        list.className = 'suggest'
        list.setAttribute('role', 'listbox')
        // The field keeps the focus (and does not blur) when a suggestion is clicked.
        list.addEventListener('mousedown', (e) => e.preventDefault())
        list.addEventListener('click', (e) => {
          const item = (e.target as HTMLElement).closest<HTMLElement>('.suggest-item')
          if (item) pick(shown[Number(item.dataset.i)].value)
        })
        // Above the dialogs it may be in.
        document.body.append(list)
        input.setAttribute('aria-expanded', 'true')
      }
      cursor = Math.min(cursor, shown.length - 1)
      list.style.minWidth = `${Math.max(input.offsetWidth, 140)}px`
      list.replaceChildren(
        ...shown.map((o, i) => {
          const item = document.createElement('div')
          item.className = 'suggest-item'
          item.setAttribute('role', 'option')
          item.dataset.i = String(i)
          const value = document.createElement('span')
          value.textContent = o.value
          item.append(value)
          if (o.label && o.label !== o.value) {
            const label = document.createElement('span')
            label.className = 'suggest-label'
            label.textContent = o.label
            item.append(label)
          }
          return item
        }),
      )
      mark()
      unplace ??= anchored(() => input, { align: 'start', gap: 2 })(list)
    }

    const key = (e: KeyboardEvent) => {
      if (e.isComposing) return
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        if (!list) open()
        if (!list) return
        e.preventDefault()
        const step = e.key === 'ArrowDown' ? 1 : -1
        cursor = cursor < 0 ? (step > 0 ? 0 : shown.length - 1) : (cursor + step + shown.length) % shown.length
        mark()
      } else if (e.key === 'Enter' && list && cursor >= 0) {
        // The pick, not the field's own Enter (which would take what is typed so far).
        e.preventDefault()
        e.stopImmediatePropagation()
        pick(shown[cursor].value)
      } else if (e.key === 'Escape' && list) {
        // Closes the list only, not the panel the field is in.
        e.stopImmediatePropagation()
        close()
      } else if (e.key === 'Tab') {
        close()
      }
    }
    const typed = () => {
      cursor = -1
      if (!picking && document.activeElement === input) open()
    }

    input.setAttribute('autocomplete', 'off')
    input.setAttribute('aria-autocomplete', 'list')
    input.addEventListener('focus', open)
    input.addEventListener('click', open)
    input.addEventListener('input', typed)
    input.addEventListener('blur', close)
    // The right-click menu takes its place.
    input.addEventListener('contextmenu', close)
    input.addEventListener('keydown', key, true)
    return () => {
      close()
      input.removeEventListener('focus', open)
      input.removeEventListener('click', open)
      input.removeEventListener('input', typed)
      input.removeEventListener('blur', close)
      input.removeEventListener('contextmenu', close)
      input.removeEventListener('keydown', key, true)
    }
  }
}
