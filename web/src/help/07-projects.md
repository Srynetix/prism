---
title: Projects and workstreams
group: Everyday work
blocks: worktrees
related: agents, worktrees, mrs, tickets
---
Where does my work stand? The **Projects** page shows each repository as a project, and in it every piece of work under way as a workstream: its worktree, its agents, its ticket, its MR and its pipeline, in one card.

## Two words
- A **project** is a repository Prism follows (`[[repos]]`): its clone, its forge, its settings.
- A **workstream** is a branch of work in a project: its worktree (or the clone itself), the agents working there, its ticket and its MR. An agent launched in the clone on `main` makes a workstream « on main », shown while it has an agent or local changes.

« Worktree » stays the git word, in the technical details and in Settings. The Agents view answers « who waits for me? »; the Projects page answers « where does my work stand? ».

[Open the Projects page](prism:projects)

## The list of projects
On the left, the projects, grouped by working folder (`[[workspaces]]`). Each one shows its signals: a workstream waiting for you, its default branch's pipeline failed, its workstreams and MRs, how far its clone is behind. A project with nothing in progress is dimmed. The band's filter (`/`) narrows the list: a project stays when its name, a branch, a ticket or an MR (`!12`) of it holds what is typed, and the band says how many do; the open project's workstreams are not filtered. The rail's **Projects** entry counts the workstreams waiting for you, and `⌘` with the digit after the Agents view's opens it.

## A project
- **The header**: its forge, its clone's branch and how far behind it is, its default branch's pipeline, its open MRs, its notes (a click opens them, filtered on it) and the ports its agents listen on; **Shell** (a shell in Prism, in its clone: tmux keeps it once the panel is closed), **Editor**, **Terminal**, the file manager and the forge.
- **New workstream**: describe the work, or pick a ticket (its **Implement**, which takes the ticket); **Start** makes a worktree (in the clone where the repository has no worktrees) and launches the agent at once, **More options…** opens the launcher prefilled. Its next ticket and its quick actions sit beside it.
- **Workstreams**, by stage: **In progress** (an agent runs there; one waiting for you is outlined), **In review** (its MR open: pipeline, open threads, approvals), **Done · to clean up** (its MR merged, its branch gone or its ticket done). **By activity** lists them all, the most recent first. **Shell**, on a workstream in a worktree or an agent's folder, opens a shell in Prism there (the clone's is the header's): the same shells as the Worktrees block's.
- **To pick up**: the last agent closed in it (**Reopen** brings its conversation back), and the workstreams where nothing runs and no MR is open: a worktree made by hand, an agent stopped (**Resume**, or **Agent here**).
- **The repository**: its clone, **Pull** (fast-forward only), and a link to its settings.

## Branches
Below the workstreams, **Branches** lists the clone's local branches, most recent first: last commit, worktree, MR, ↑↓ against its origin branch, +/− against the base, and a verdict. **Safe** (green): done, with nothing to lose: its MR merged (a squash too), gone from origin, or already in the base, with no commit on no remote branch and no change in its worktree. A protected branch, the clone's, or one an agent works on is never deleted.

On the computer, each branch offers **Worktree** (one next to the clone, without an agent), **Switch the clone**, **Agent here** in its worktree, and **Delete** at a second click, its worktree with it. What would lose commits or changes only goes by **Force**, which says what is lost. Nothing is deleted on the forge.

**Tidy up** reads origin again and prunes it (`fetch --prune`, `worktree prune`), then lists the safe branches, ticked: **Delete** removes them, each checked again, and says what was deleted and what was kept, and why. On the phone, branches are read only, and **Tidy up** asks for the passkey. The Cleanup tab counts each project's safe branches, with a **Tidy up** of its own.

## Cleaning up
A workstream done shows the same verdict as the Worktrees block: **Nothing to lose**, or what a removal would lose. **Remove the worktree** takes two clicks; when work would be lost, the second click says what. **Archive** takes it out of the list until a new commit or change.

The **Cleanup** tab, at the top of the list, is the Worktrees block over every project: sizes, verdicts, removal and archiving in bulk. It replaces a page holding only the Worktrees block: such a page can go (**Edit layout**); the Worktrees block itself stays on the pages that use it.

[Open the Cleanup tab](prism:projects/cleanup)

## From elsewhere
- The status bar's **Repos** menu: **Open the project** at the head of each repository.
- The Agents view **by project**: the folder button beside a project's **+** opens its page.
- On the phone, **By project** in Agents: a project's name, or the repository on an agent's card, opens its page, its workstreams as cards and **New workstream** at the bottom. **By status** stays the default.
