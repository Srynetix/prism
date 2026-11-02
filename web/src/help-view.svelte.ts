/**
 * The Help view's open state, kept apart from help.svelte.ts so that Settings and Agents can
 * close it without importing the guides.
 */
export const helpView = $state({ open: false, guide: 'welcome', query: '', zoom: null as { src: string; alt: string } | null })

export const closeHelp = () => (helpView.open = false)
