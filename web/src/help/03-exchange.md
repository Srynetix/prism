---
title: Exchange in the macOS Calendar
group: Start
related: notes, setup, settings
---
On a Mac, the Today block reads your **Exchange** or **Microsoft 365** meetings through the macOS Calendar: add the account once in System Settings, and Prism reads what Calendar has synced, locally. No Entra ID application, no token in Prism, no network call: the Mac syncs with Exchange, and Prism reads the Mac.

## What Prism reads
Prism reads the Calendar's events through EventKit, the macOS calendar framework, read-only, every minute and when it opens. Of each event it keeps only what the block shows: the title, the times, the place, the calendar, your answer, whether it is canceled or "free", the number of attendees, and the video call link (Teams, Zoom or Meet), found in the event's link, place or invitation text. The attendees and the invitation text are read when you open an event, and never kept.

Prism never creates, changes, deletes nor answers anything: EventKit does not allow answering an invitation. **Respond in Calendar** opens the event in Calendar, where you answer, and Calendar sends your answer to Exchange.

> [!IMPORTANT]
> **Microsoft 365 and EWS.** Calendar talks to Exchange through Exchange Web Services (EWS), which Microsoft has been turning off in Exchange Online since 1 October 2026, tenant by tenant. It is due to stop for good on 1 April 2027. Apple has announced a macOS 27 update that moves Calendar and Mail to Microsoft Graph, with no date given yet. Until then, in an organization where EWS is off, the Microsoft 365 account stops syncing. Calendar keeps the events it already had, and Prism shows them **without warning that they are stale**. See *When it does not work* below. An on-premises Exchange server is not affected.

## 1. Add the account to the Mac
1. Open **System Settings › Internet Accounts**, then **Add Account…** and **Microsoft Exchange**.
2. Type your name and your work email address, then **Sign In**. Microsoft's sign-in page opens: your password, then the second factor your organization asks for (Authenticator, SMS…).
   - If the Mac does not find the server, choose **Configure Manually** and give the server address: `outlook.office365.com` for Microsoft 365, or the one your IT gives you for an on-premises server.
3. In the list of apps, check **Calendars**. Mail, Contacts, Reminders and Notes are up to you, Prism does not use them. Then **Done**.

Open **Calendar**: the account appears in the sidebar with its calendars, usually one named **Calendar** (or **Calendrier**, in the language of the mailbox). The first sync can take a minute.

> [!NOTE]
> An account already added for Mail only: in **System Settings › Internet Accounts**, click it and turn on **Calendars**.

## 2. Let Prism read the Calendar
1. In [Settings › Work › Calendar](prism:settings/calendar), turn on **Agenda** and leave **Provider** on `macos`, the default.
2. **Save to config.toml** (`⌘S`), then **Restart Prism** if you changed something: the agenda is read at launch.
3. macOS asks whether **Prism** may access your calendars: **Allow Full Access**. Prism needs full access to *read* the events. Access limited to adding events lets it read nothing.

Then add the **Today** block to a page with **Edit layout › + Block**.

> [!NOTE]
> Access is granted to the built app (`Prism.app`). Run with `pnpm dev`, and macOS asks for the terminal that launched it instead, so Prism does not appear in the Calendars list. The app is signed ad hoc, so macOS may ask again after a rebuild.

## 3. Choose the calendars
**Settings › Work › Calendar › Calendars** lists every calendar of the Mac, from every account: Exchange, iCloud, Google, subscriptions, Birthdays, Holidays. Leave all of them unchecked and Prism reads all of them. Check some and it reads only those. Unchecking a calendar in Calendar's sidebar only hides it in Calendar: Prism still reads it, so choose here.

Calendars are chosen **by name**, case aside. Two accounts often both have a calendar named **Calendar** (Exchange and iCloud, say): checking that name reads both. To keep only the Exchange one, rename the other in Calendar (**Get Info**, on the calendar in the sidebar), then check the Exchange one by its name.

Shared calendars and delegated ones (a manager's, a meeting room's) appear once Calendar shows them. Add a delegation in **Calendar › Settings › Accounts**, the Exchange account, **Delegation**.

## Freshness
Calendar syncs with Exchange on its own, according to **Calendar › Settings › Accounts › Refresh Calendars** (**Push**, or every 5, 15, 30 minutes…), and Prism reads the result every minute. A meeting added in Outlook therefore shows up after Calendar's sync, then within a minute. To force a sync, open Calendar and press `⌘R`.

The reminder 5 minutes before a meeting, the countdown and the **Join** button follow what Calendar knows: an invitation that Calendar has not synced yet triggers nothing.

## When it does not work
- **The Today block asks for access**, or Prism is missing from **System Settings › Privacy & Security › Calendars**: access was refused, or the app was rebuilt. Turn **Prism** on in that list, with full access. If it is not listed, run `tccutil reset Calendar app.prism.desktop` in a terminal, then restart Prism so that macOS asks again.
- **No Exchange meeting, the others are there**: in Calendar, check that the account's calendars are listed. If they are not, turn on **Calendars** for the account (step 1). If they are, check that **Calendars** in Prism's Settings does not leave them out.
- **The meetings stopped changing** (a meeting moved or canceled in Outlook still shows as before, new ones never appear), or Calendar marks the account with a ⚠ or asks for the password again and again: if it is a Microsoft 365 account, EWS is probably off in your organization (see the warning above). Check in Outlook on the web that the meeting really changed. Then:
  - ask your IT whether EWS is still allowed for Apple's Mail and Calendar, as Microsoft lets them allow it until April 2027;
  - or install the macOS update that moves Calendar to Microsoft Graph, once Apple ships it;
  - meanwhile, Outlook for Mac keeps working, but its calendar is not part of the macOS Calendar, so Prism does not see it.
- **"Your organization requires…"**, or the sign-in is refused: your organization may block the Mac's native apps (conditional access). Ask your IT.
- **No Join button**: the video link is neither in the event's link, nor in its place, nor in its invitation text, or it is not a Teams, Zoom or Meet link. Open the event and its invitation text to check.
- **A meeting appears twice**: the same invitation is in two calendars read (yours and a delegated one, say). Uncheck one of them in **Calendars**.

[Open Settings › Work › Calendar](prism:settings/calendar)
[Open Settings › Work › Calendar › macOS Calendar](prism:settings/calendar-macos)
