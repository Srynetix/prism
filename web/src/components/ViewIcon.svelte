<script lang="ts">
  /** A page's icon (`VIEW_ICONS`), in the rail, its header and its menus; an unknown icon takes the dashboard's. */
  let { id }: { id: string } = $props()
</script>

<svg viewBox="0 0 24 24" aria-hidden="true">
  {#if id === 'tickets'}<path d="M4 6h16M4 12h11M4 18h7" /><path d="m15.5 19.5 1-3.5 4-4 2.5 2.5-4 4z" />
  {:else if id === 'notes'}<path d="M5 3.5h10l4 4V20.5H5z" /><path d="M14.5 3.5v4.5H19M8.5 12h7M8.5 16h5" />
  {:else if id === 'chart'}<path d="M4 20V4M4 20h16" /><path d="M8 16v-4M12 16V8M16 16v-6" />
  {:else if id === 'star'}<path d="m12 3.2 2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17.2l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9z" />
  {:else if id === 'calendar'}<rect x="3.5" y="5" width="17" height="15.5" rx="2" /><path d="M3.5 10h17M8 3v4M16 3v4" />
  {:else if id === 'shield'}<path d="M12 3 4.5 6v5.5c0 4.6 3.2 8 7.5 9.5 4.3-1.5 7.5-4.9 7.5-9.5V6z" /><path d="m9 12 2.2 2.2L15.5 10" />
  {:else if id === 'music'}<path d="M9 18V5.5l11-2V16" /><circle cx="6.5" cy="18" r="2.5" /><circle cx="17.5" cy="16" r="2.5" />
  {:else if id === 'ci'}<circle cx="6" cy="6" r="2.2" /><circle cx="18" cy="12" r="2.2" /><circle cx="6" cy="18" r="2.2" /><path d="M8.2 6H12a3 3 0 0 1 3 3v.8M8.2 18H12a3 3 0 0 0 3-3v-.8" />
  {:else if id === 'agents'}<rect x="2.5" y="4" width="19" height="16" rx="2.5" /><path d="m7 10 3 2.5L7 15M12.5 15H17" />
  {:else if id === 'worktrees'}<path d="M3 7.5A1.5 1.5 0 0 1 4.5 6H9l2 2.5h8.5A1.5 1.5 0 0 1 21 10v8.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5z" /><circle cx="9.5" cy="15.5" r="1.4" /><circle cx="15" cy="12.5" r="1.4" /><path d="M9.5 14v-1.5a1.5 1.5 0 0 1 1.5-1.5h2.6" />
  {:else if id === 'data'}<ellipse cx="12" cy="5.5" rx="7.5" ry="2.8" /><path d="M4.5 5.5v13c0 1.5 3.4 2.8 7.5 2.8s7.5-1.3 7.5-2.8v-13M4.5 12c0 1.5 3.4 2.8 7.5 2.8s7.5-1.3 7.5-2.8" />
  {:else if id === 'infra'}<path d="M12 2.8 20.5 7.5v9L12 21.2 3.5 16.5v-9z" /><path d="M12 12 20.5 7.5M12 12 3.5 7.5M12 12v9.2" />
  {:else}<rect x="3" y="3" width="7.5" height="9" rx="1.6" /><rect x="13.5" y="3" width="7.5" height="5" rx="1.6" /><rect x="13.5" y="11" width="7.5" height="10" rx="1.6" /><rect x="3" y="15" width="7.5" height="6" rx="1.6" />{/if}
</svg>
