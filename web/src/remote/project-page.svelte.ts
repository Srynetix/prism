/**
 * The project whose page the phone shows over the agents (`RemoteProjectPage`), from « By project »
 * or an agent's card. Its own state, so that RemoteApp's Back closes it like any other panel.
 */
export const projectPage = $state({ repo: null as string | null })

export const openProjectPage = (repo: string) => (projectPage.repo = repo)
export const closeProjectPage = () => (projectPage.repo = null)
