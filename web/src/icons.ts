import type { Component } from 'svelte'
import Activity from '@lucide/svelte/icons/activity'
import Cpu from '@lucide/svelte/icons/cpu'
import AlarmClock from '@lucide/svelte/icons/alarm-clock'
import Moon from '@lucide/svelte/icons/moon'
import ArrowDown from '@lucide/svelte/icons/arrow-down'
import ArrowLeft from '@lucide/svelte/icons/arrow-left'
import ArrowLeftRight from '@lucide/svelte/icons/arrow-left-right'
import ArrowUp from '@lucide/svelte/icons/arrow-up'
import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right'
import Asterisk from '@lucide/svelte/icons/asterisk'
import Ban from '@lucide/svelte/icons/ban'
import Flag from '@lucide/svelte/icons/flag'
import Mail from '@lucide/svelte/icons/mail'
import Columns2 from '@lucide/svelte/icons/columns-2'
import Save from '@lucide/svelte/icons/save'
import ChartPie from '@lucide/svelte/icons/chart-pie'
import ChartSpline from '@lucide/svelte/icons/chart-spline'
import Command from '@lucide/svelte/icons/command'
import SquareSquare from '@lucide/svelte/icons/square-square'
import SquareChevronRight from '@lucide/svelte/icons/square-chevron-right'
import CheckCheck from '@lucide/svelte/icons/check-check'
import Check from '@lucide/svelte/icons/check'
import ChevronDown from '@lucide/svelte/icons/chevron-down'
import ChevronUp from '@lucide/svelte/icons/chevron-up'
import ChevronLeft from '@lucide/svelte/icons/chevron-left'
import ChevronRight from '@lucide/svelte/icons/chevron-right'
import ChevronsRight from '@lucide/svelte/icons/chevrons-right'
import Circle from '@lucide/svelte/icons/circle'
import CircleDot from '@lucide/svelte/icons/circle-dot'
import CirclePlus from '@lucide/svelte/icons/circle-plus'
import CalendarDays from '@lucide/svelte/icons/calendar-days'
import Clock from '@lucide/svelte/icons/clock'
import Copy from '@lucide/svelte/icons/copy'
import CornerDownRight from '@lucide/svelte/icons/corner-down-right'
import CornerLeftUp from '@lucide/svelte/icons/corner-left-up'
import Disc3 from '@lucide/svelte/icons/disc-3'
import Ellipsis from '@lucide/svelte/icons/ellipsis'
import Eye from '@lucide/svelte/icons/eye'
import Download from '@lucide/svelte/icons/download'
import EyeOff from '@lucide/svelte/icons/eye-off'
import GitBranch from '@lucide/svelte/icons/git-branch'
import FileDiff from '@lucide/svelte/icons/file-diff'
import FolderTree from '@lucide/svelte/icons/folder-tree'
import Globe from '@lucide/svelte/icons/globe'
import FolderGit2 from '@lucide/svelte/icons/folder-git-2'
import Folder from '@lucide/svelte/icons/folder'
import Code from '@lucide/svelte/icons/code'
import BrushCleaning from '@lucide/svelte/icons/brush-cleaning'
import Trash from '@lucide/svelte/icons/trash'
import GitCompareArrows from '@lucide/svelte/icons/git-compare-arrows'
import GitMerge from '@lucide/svelte/icons/git-merge'
import Kanban from '@lucide/svelte/icons/kanban'
import GitPullRequest from '@lucide/svelte/icons/git-pull-request'
import GripVertical from '@lucide/svelte/icons/grip-vertical'
import Hash from '@lucide/svelte/icons/hash'
import Lock from '@lucide/svelte/icons/lock'
import LockOpen from '@lucide/svelte/icons/lock-open'
import Layers from '@lucide/svelte/icons/layers'
import ArrowDownAZ from '@lucide/svelte/icons/arrow-down-a-z'
import Hexagon from '@lucide/svelte/icons/hexagon'
import Image from '@lucide/svelte/icons/image'
import ListOrdered from '@lucide/svelte/icons/list-ordered'
import Maximize2 from '@lucide/svelte/icons/maximize-2'
import MessageSquare from '@lucide/svelte/icons/message-square'
import Mic from '@lucide/svelte/icons/mic'
import MicOff from '@lucide/svelte/icons/mic-off'
import Minimize2 from '@lucide/svelte/icons/minimize-2'
import Minus from '@lucide/svelte/icons/minus'
import MonitorPlay from '@lucide/svelte/icons/monitor-play'
import Music from '@lucide/svelte/icons/music'
import Pause from '@lucide/svelte/icons/pause'
import Pin from '@lucide/svelte/icons/pin'
import Pencil from '@lucide/svelte/icons/pencil'
import Play from '@lucide/svelte/icons/play'
import Plus from '@lucide/svelte/icons/plus'
import Podcast from '@lucide/svelte/icons/podcast'
import Radio from '@lucide/svelte/icons/radio'
import RectangleHorizontal from '@lucide/svelte/icons/rectangle-horizontal'
import RefreshCcw from '@lucide/svelte/icons/refresh-ccw'
import RefreshCw from '@lucide/svelte/icons/refresh-cw'
import Rocket from '@lucide/svelte/icons/rocket'
import RotateCw from '@lucide/svelte/icons/rotate-cw'
import Rows3 from '@lucide/svelte/icons/rows-3'
import ScrollText from '@lucide/svelte/icons/scroll-text'
import Search from '@lucide/svelte/icons/search'
import CircleHelp from '@lucide/svelte/icons/circle-question-mark'
import ShieldAlert from '@lucide/svelte/icons/shield-alert'
import ShieldCheck from '@lucide/svelte/icons/shield-check'
import ShipWheel from '@lucide/svelte/icons/ship-wheel'
import Settings from '@lucide/svelte/icons/settings'
import SkipBack from '@lucide/svelte/icons/skip-back'
import SkipForward from '@lucide/svelte/icons/skip-forward'
import Send from '@lucide/svelte/icons/send'
import Square from '@lucide/svelte/icons/square'
import SquareTerminal from '@lucide/svelte/icons/square-terminal'
import SquareCheck from '@lucide/svelte/icons/square-check'
import Star from '@lucide/svelte/icons/star'
import Tag from '@lucide/svelte/icons/tag'
import Timer from '@lucide/svelte/icons/timer'
import TriangleAlert from '@lucide/svelte/icons/triangle-alert'
import Undo2 from '@lucide/svelte/icons/undo-2'
import User from '@lucide/svelte/icons/user'
import Volume1 from '@lucide/svelte/icons/volume-1'
import Volume2 from '@lucide/svelte/icons/volume-2'
import VolumeX from '@lucide/svelte/icons/volume-x'
import X from '@lucide/svelte/icons/x'
import Zap from '@lucide/svelte/icons/zap'

/**
 * Interface icons (Lucide), by role rather than by drawing: the data (menus, steps
 * of a deployment, audio sources) carry a name, `Icon.svelte` draws it. Add a role here
 * before using it.
 */
export const ICONS = {
  // Actions
  refresh: RefreshCw, // re-read, rescan (↻)
  retry: RotateCw, // retry a pipeline or a job (⟳)
  play: Play, // run a manual job, playback (▶ ▷)
  pause: Pause,
  stop: Square, // stop a pipeline (■)
  prev: SkipBack,
  next: SkipForward,
  send: Send, // a note sent to the selected agent
  close: X, // close, remove, clear (× ✕)
  copy: Copy, // copy (⧉)
  hash: Hash, // copy an identifier (#)
  lock: Lock, // a ticket waiting on others
  unlock: LockOpen, // its blockers all done
  group: Layers, // group a list (notes by tag)
  sortAz: ArrowDownAZ, // sort a list by name
  search: Search, // filter (⌕)
  help: CircleHelp, // in-app Help (?)
  edit: Pencil, // draft, daily (✎)
  add: CirclePlus, // add to sprint (⊕)
  plus: Plus, // create (+ New)
  validate: CheckCheck, // an agent validating (agents rail)
  external: ArrowUpRight, // open elsewhere (↗)
  expand: Maximize2, // full screen, zoom (⤢)
  shrink: Minimize2, // exit full screen (⤡)
  more: Ellipsis, // menu ⋯
  settings: Settings, // Settings view
  terminal: SquareTerminal, // Agents view, an agent's terminal
  activity: Activity, // Agents view, what an agent did
  chart: ChartSpline, // a pod's CPU and memory over time
  cpu: Cpu, // the System block: the machine's CPU, memory, network
  grip: GripVertical, // drag handle (⋮⋮)
  undo: Undo2, // reset to "new" (↺)
  show: Eye,
  hide: EyeOff,
  download: Download, // a file or a zip saved
  // States
  check: Check, // succeeded, done, approved (✓)
  fail: X, // failed (✗)
  warning: TriangleAlert, // to fix (invalid setting)
  running: CircleDot, // in progress, live (●)
  pending: Circle, // upcoming (○)
  canceled: Ban, // canceled (⊘)
  skipped: ChevronsRight, // skipped (»)
  none: Minus, // not applicable, collapsed (–)
  checked: SquareCheck, // checked box (☑)
  unchecked: Square, // empty box (☐)
  // Navigation
  chevronDown: ChevronDown, // expanded (▾)
  chevronUp: ChevronUp, // a diff's lines above (▴)
  chevronRight: ChevronRight, // collapsed, caret (▸ ▶)
  chevronLeft: ChevronLeft, // previous day (‹)
  back: ArrowLeft, // go back to the previous (←)
  down: ArrowDown,
  up: ArrowUp,
  subtask: CornerDownRight, // subtask (↳)
  parent: CornerLeftUp, // to the parent (↑ parent)
  // Domain
  helm: ShipWheel, // ⎈
  argocd: RefreshCcw, // ArgoCD sync (⟳)
  kube: Hexagon, // pods, nodes (⬢)
  review: Eye, // reviews (◉)
  sync: ArrowLeftRight, // MRs to resync (⇄)
  mr: GitPullRequest, // open an MR (⇄)
  branch: GitBranch, // ⑂ ⎇
  worktree: FolderGit2, // Worktrees view
  folder: Folder, // Finder
  code: Code, // editor
  files: FolderTree, // files of an agent's folder
  diff: FileDiff, // diff of an agent's folder
  web: Globe, // an agent's Web view
  category: Tag, // an agent's category
  label: Tag, // an MR's labels
  trash: Trash, // delete (a worktree)
  prune: BrushCleaning, // clean up, git worktree prune
  rebase: GitCompareArrows, // ⤴
  merge: GitMerge,
  kanban: Kanban, // tickets, boards
  queue: ListOrdered, // merge queue (⇶)
  automerge: Zap, // ⚡ ⇥
  quick: Zap, // a quick action: an agent in one click
  security: ShieldAlert, // ⛨
  secure: ShieldCheck, // nothing to fix
  logs: ScrollText, // ≡
  mail: Mail, // Mail block
  flag: Flag, // a flagged message
  timer: Timer, // time entry (⏱)
  user: User, // 👤
  claude: Asterisk, // ✳
  commandcode: Command, // ⌘, Command Code
  opencode: SquareSquare, // ⧈, OpenCode
  codex: SquareChevronRight, // ›_, Codex
  rocket: Rocket, // deployment in progress (🚀)
  clock: Clock, // meeting (◷)
  calendar: CalendarDays, // date picker
  alarm: AlarmClock, // reminder (⏰)
  snooze: Moon, // put an alert off (⏾)
  pin: Pin, // sticky note
  split: Columns2, // two files side by side (◫)
  save: Save, // a file written back
  sprint: ChartPie, // sprint day (◔)
  comment: MessageSquare, // discussion threads (💬)
  image: Image, // 🖼
  star: Star, // organizer (★)
  // Audio
  music: Music, // ♪
  volume: Volume1, // 🔉
  // The Sound block: the output and the microphone
  speaker: Volume2,
  speakerOff: VolumeX,
  mic: Mic,
  micOff: MicOff,
  artwork: Disc3, // missing artwork (◎)
  radio: Radio, // stations (≋)
  video: MonitorPlay, // YouTube videos (▶)
  podcast: Podcast, // RSS feeds (▤)
  // Block layout
  blockNormal: Rows3, // ▤
  blockShort: RectangleHorizontal, // ▭
} satisfies Record<string, Component<any>>

export type IconName = keyof typeof ICONS
