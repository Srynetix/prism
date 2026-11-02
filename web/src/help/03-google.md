---
title: Signing in to Google (Gmail, Google Calendar)
group: Start
related: setup, notes, settings
---
The Mail block can read your **Gmail** inbox, and the Today block your **Google Calendar**, on any system: Linux and Windows included, where there is no macOS Calendar. Both sign in with one Google account per profile, through an OAuth client of your own, created once in Google Cloud in about ten minutes. Nothing is shared with anyone else: the client is yours, and so is the sign-in.

## What Prism reads
Prism asks only for the permissions of the sources you turned on, all of them read-only:

| Source | Permission | What Prism reads |
|---|---|---|
| Gmail (Mail block) | `gmail.metadata` | The labels and the headers of your messages: the sender, the subject, the date, unread and starred. Never a body, a snippet nor an attachment. |
| Google Calendar (Today block) | `calendar.calendarlist.readonly`, `calendar.events.readonly` | The list of your calendars, and their events. Prism keeps only what the block shows; an event's description is read when you open it, and never kept. |

Prism never sends, deletes, archives, marks as read or answers anything: it cannot. A message opens in Gmail on the web; an invitation is answered in Google Calendar on the web.

## 1. Create a Google Cloud project
Open [console.cloud.google.com](https://console.cloud.google.com) with the Google account you want to read, a personal one is fine. In the project picker at the top, **New project**, name it `Prism`, **Create**, then select it. Google Cloud may ask you to accept its terms: no billing account is needed, these APIs are free at this usage.

## 2. Turn on the APIs
In **APIs & Services › Library**, search for and **Enable** the ones you will use:

- **Gmail API**, for the Mail block;
- **Google Calendar API**, for the Today block.

## 3. Set up the consent screen
In **APIs & Services › OAuth consent screen** (the console may call it **Google Auth Platform**), **Get started**:

1. **App information**: the app name, `Prism`, and your email as the support email.
2. **Audience**: **External**. A personal Gmail account has no other choice. With a Google Workspace account (your company's), **Internal** is better if your admin allows it: no warning when signing in, and no expiry.
3. **Contact information**: your email. Accept the policy, **Create**.
4. **Data access › Add or remove scopes**: add `.../auth/gmail.metadata`, `.../auth/calendar.calendarlist.readonly` and `.../auth/calendar.events.readonly` (those you use), **Update**, **Save**. Google then shows them on its consent page.
5. **Branding**: complete the page beyond the name and the support email, or Google may refuse to publish the app:
   - **App logo**: optional. Leave it out unless you need it: Google may then require a brand verification before showing it.
   - **App domain**: the **Application home page**, the **Privacy policy link** and the **Terms of service link**. For your own use, any pages you control do: your Prism fork or a page of your site. They are shown on Google's consent page, and only to you.
   - **Authorized domains**: the domain of those links (`github.com`, `example.org`…), added automatically or by **Add domain**.
   - **Developer contact information**: your email.

   Then **Save**.
6. **Audience › Publishing status**: **Publish app**, then **Confirm**: the app goes **In production**.

> [!IMPORTANT]
> Do not leave the app in **Testing**. Google then expires your sign-in after 7 days, and Prism would sign you out every week. In production without verification, the app keeps working for you: Google shows a warning when you sign in (see step 5 below), and limits it to 100 users, which is more than you need. Do not submit the app for verification: it is meant for apps published to the public.

## 4. Create the OAuth client
In **APIs & Services › Credentials** (or **Clients** in Google Auth Platform), **Create credentials › OAuth client ID**:

- **Application type**: **Desktop app**. Not "Web application": a desktop client is the one that accepts Prism's answer on your own computer (`http://127.0.0.1`, any port), and has no redirect URI to configure.
- **Name**: `Prism`, then **Create**.

Google shows the **Client ID** (`…apps.googleusercontent.com`) and the **Client secret** (`GOCSPX-…`). Keep the window open, or **Download JSON**.

## 5. Sign in from Prism
In Settings, **Mail** and **Calendar** each list their providers as sub-pages: **Microsoft 365** and **Gmail** under Mail, **macOS Calendar** and **Google Calendar** under Calendar. The one not chosen is dimmed, marked *inactive*. **Gmail** and **Google Calendar** show the same Google account: set it up in either.

1. Choose what you want to read:
   - Gmail: in [Settings › Work › Mail](prism:settings/mail), turn on **Mail** and set **Provider** to `gmail` (or **Make Gmail the mail provider** on the Gmail page);
   - Google Calendar: in [Settings › Work › Calendar](prism:settings/calendar), turn on **Agenda** and set **Provider** to `google`.
2. In [Mail › Gmail](prism:settings/mail-gmail) (or [Calendar › Google Calendar](prism:settings/calendar-google)), paste the **Client ID**, and the **Client secret** in its field, then **Save to keychain**.
3. **Save to config.toml** (`⌘S`), then **Restart Prism**: the providers are read at launch.
4. Back on that page, **Sign in with Google**. Your browser opens on Google's page:
   - choose the account (the one whose project holds the client, or any account if the app is in production);
   - Google warns **"Google hasn't verified this app"**: it is your own app. Click **Advanced**, then **Go to Prism (unsafe)**;
   - **check every box** Google offers, then **Continue**.
5. The tab says *Prism is signed in to Google*: close it. Settings shows **Signed in**, and the blocks read at once.

Then add the blocks to a page with **Edit layout › + Block**: **Mail**, and **Today** for the agenda.

> [!NOTE]
> Prism waits 5 minutes for Google's answer, on your own computer: sign in in a browser of the machine running Prism.

## The calendars read
Once signed in, **Settings › Work › Calendar › Calendars** lists your Google calendars (reopen Settings if the list is empty). None checked: the calendars checked in Google Calendar's own list. Checked: only those, by name. A calendar you renamed in Google Calendar reads under your name for it.

## Where the secrets go
The client secret and the refresh token go into the system keychain, never into `config.toml`: the macOS Keychain, the Windows Credential Manager, or Secret Service on Linux (GNOME Keyring, KWallet). Their entries are `prism-google-secret` and `prism-google`, with `-<profile>` after them for a profile other than the default one. Each profile has its own Google account: paste the same client in two profiles to sign in with two accounts.

On Linux without a Secret Service (a minimal desktop, a window manager alone), install and start `gnome-keyring` first. `prism google-secret` also reads the client secret on its standard input, for the active profile: run it, paste the secret, then `Ctrl-D`.

## Turning on the second source later
If you signed in for Gmail, then turn on Google Calendar (or the other way round), Prism has not been granted the new permission yet: the block says *Google did not grant access to Google Calendar*. **Sign in again** in Settings › Work › Mail › Gmail (or Calendar › Google Calendar), and check every box.

## Signing out
**Sign out** (two clicks) revokes the sign-in at Google and removes the refresh token from the keychain; the client secret stays. You can also remove Prism's access from your Google account, at [myaccount.google.com/permissions](https://myaccount.google.com/permissions): Prism then says it is signed out at its next read.

## When it does not work
- **"Access blocked: Prism has not completed the Google verification process"**: the app is still in Testing, and the account is not one of its test users. Publish it (step 3.6), or add the account under **Audience › Test users**.
- **Signed out every week** (*Token has been expired or revoked*): the app is still in Testing. Publish it, then sign in again.
- **"Google did not grant access to …"**: a box was left unchecked on Google's page, or a source was turned on since. Sign in again and check every box.
- **"Gmail API has not been used in project … or it is disabled"** (or Google Calendar API): turn it on (step 2), wait a minute, and Prism reads again by itself.
- **"invalid_client"** or **"redirect_uri_mismatch"**: the client ID or secret is wrong, or the client is not of the **Desktop app** type. Create a Desktop app client and paste both again.
- **"no Google client secret in the keychain"**: paste it in Settings › Work › Mail › Gmail (or Calendar › Google Calendar).
- **A company account is refused**: your Google Workspace admin may block apps they have not approved. Ask them, or create the project in your company's organization with the **Internal** audience.
- **Nothing happens on Sign in**: no default browser opened. Set one (`xdg-settings set default-web-browser …` on Linux), then sign in again.

[Open Settings › Work › Mail › Gmail](prism:settings/mail-gmail)
[Open Settings › Work › Calendar › Google Calendar](prism:settings/calendar-google)
