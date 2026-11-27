/**
 * The MRs lit while a dependency chip or a graph's node is pointed at: that MR and its
 * lineage (`lineage`), which the MR rows highlight.
 */
export const depHover = $state<{ lit: Set<string> }>({ lit: new Set() })
