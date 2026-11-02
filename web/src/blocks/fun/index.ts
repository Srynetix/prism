import type { BlockDef } from '../block'
import { t } from '../../i18n.svelte'

/**
 * Blocks of no use at all, for the fun of it: nothing to read nor to turn on, only a page to put
 * them on (Edit layout › + Block). They live in the window alone; what they remember stays in
 * this profile's layout.
 */
const fun = (id: string, label: string, purpose: string, points: string[]) =>
  ({ id, label, category: 'Fun', help: { purpose, points, source: t('Nothing: it lives in the window, and keeps its few memories in your layout.') } }) as const

export const shipit = {
  ...fun('shipit', t('Ship it?'), t('Should you deploy right now? An honest answer, from the clock and the CI.'), [
    t('Friday afternoon, the evening, the weekend, the minutes before a meeting: no.'),
    t('A red pipeline on a protected branch, or an alert in red: not now either.'),
    t('Otherwise: yes, go. Prism takes no responsibility for this answer.'),
  ]),
  id: 'shipit' as const,
} satisfies BlockDef

export const quotes = {
  ...fun('quotes', t('Quotes'), t('A quote about programming, and who said it: Knuth, Dijkstra, Hopper, Brooks…'), [
    t('A new one every hour on its own, or at each click on Another.'),
    t('Copy puts it on the clipboard, with its author.'),
  ]),
  id: 'quotes' as const,
  // It gave developer excuses, at first.
  aliases: ['excuses'],
} satisfies BlockDef

export const pet = {
  ...fun('pet', t('Pet'), t('A virtual companion living on your dashboard: look after it, and it follows your work.'), [
    t('Three needs that run down with time, even with Prism closed: food, energy and fun. Feed it, play with it (it jumps after the ball), pet it, turn its light off to let it sleep.'),
    t('It grows with you: each merged MR and each care gives experience; it levels up, and changes look as it grows (baby, young, grown-up, legend).'),
    t('It reacts to your work: sick with a red protected pipeline, worried by red alerts, it tells you when an agent waits for you or a meeting is about to start, and celebrates a merge.'),
    t('It sleeps at night and at the weekend. Click its name to rename it. Its eyes follow your pointer.'),
    t('With the Day trail, it grows into your work: grown up, it takes the form of what you did most these two weeks (Reviewer, Shipper, Fixer, Night Owl, Architect); trophies put objects in its room (🏆 lists them); on Friday afternoon it writes a diary page on your week (📔).'),
    t('Its friends, the running agents, sit in its room: the one waiting for you waves, a click opens it. Fetch (🦴) brings back the next thing waiting for you: an agent, a review, one of your MRs to act on.'),
    t('Its window follows the hour and the season, with a few special days (its birthday, the sprint’s last day, Friday afternoon). Each milestone is a photo in its album (📷). Off its page, it waits in the status bar.'),
  ]),
  id: 'pet' as const,
  status: { label: t('Pet'), side: 'right', after: 'usage' },
  settings: {
    parent: 'interface',
    node: { id: 'pet', label: t('Pet'), sections: ['pet'] },
    sections: [
      {
        id: 'pet', source: 'pet', label: t('Pet'), table: '[pet]', path: ['pet'],
        intro: t('What the Pet block does beyond its needs and cares. Forms, trophies and the diary read the Day trail’s journal (Settings › Work › Day trail). Its memories stay in this profile’s layout.'),
        fields: [
          { key: 'forms', label: t('Forms'), hint: t('Grown up (level 6), it takes the form of what you did most over the last 14 days, at most once a week.'), kind: 'toggle' },
          { key: 'trophies', label: t('Trophies'), hint: t('Milestones (ten merges, a green week, fifty reviews…) unlock objects in its room.'), kind: 'toggle' },
          { key: 'diary', label: t('Diary'), hint: t('On Friday afternoon (or the week after), it writes a page on your week.'), kind: 'toggle' },
          { key: 'album', label: t('Album'), hint: t('A photo at each milestone: its hatching, each stage, each form, each trophy. The last one hangs on its wall.'), kind: 'toggle' },
          { key: 'friends', label: t('Friends'), hint: t('The running agents sit in its room, coloured by their CLI; the one waiting for you waves. A click opens it.'), kind: 'toggle' },
          { key: 'fetch', label: t('Fetch'), hint: t('A care that brings back the next thing waiting for you (an agent, a review, one of your MRs to act on); done within the hour, +5 XP.'), kind: 'toggle' },
          { key: 'seasons', label: t('Seasons'), hint: t('Its window follows the hour and the season, with a few special days: its birthday, the sprint’s last day, Friday afternoon, holidays.'), kind: 'toggle' },
          { key: 'status_bar', label: t('In the status bar'), hint: t('While its block is on a page but not the one shown, a small Pet in the status bar: a click, its needs and cares.'), kind: 'toggle' },
        ],
      },
    ],
  },
} satisfies BlockDef
