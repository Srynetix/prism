/**
 * My sprint's figures are the core's (`crates/blocks/src/trail/retro.rs`, read with `retro_get`):
 * here, what the window shows them with.
 */

/** `45 min`, `3 h`, `3 h 20`. */
export const hours = (min: number) => (min < 60 ? `${Math.round(min)} min` : `${Math.floor(min / 60)} h${min % 60 ? ` ${String(Math.round(min % 60)).padStart(2, '0')}` : ''}`)
