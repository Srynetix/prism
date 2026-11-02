---
title: Exchange dans le Calendrier macOS
group: Start
related: notes, setup, settings
---
Sur un Mac, le bloc Aujourd’hui lit vos réunions **Exchange** ou **Microsoft 365** via le Calendrier de macOS : ajoutez le compte une fois dans les Réglages Système, et Prism lit ce que Calendrier a synchronisé, en local. Pas d’application Entra ID, pas de jeton dans Prism, pas d’appel réseau : le Mac se synchronise avec Exchange, et Prism lit le Mac.

## Ce que Prism lit
Prism lit les événements du Calendrier via EventKit, le framework de calendrier de macOS, en lecture seule, chaque minute et à l’ouverture. De chaque événement il ne garde que ce que le bloc montre : le titre, les horaires, le lieu, le calendrier, votre réponse, s’il est annulé ou « disponible », le nombre de participants, et le lien de visio (Teams, Zoom ou Meet), trouvé dans le lien de l’événement, son lieu ou le texte de l’invitation. Les participants et le texte de l’invitation sont lus quand vous ouvrez un événement, et jamais conservés.

Prism ne crée, ne modifie, ne supprime et ne répond jamais à rien : EventKit ne permet pas de répondre à une invitation. **Répondre dans Calendrier** ouvre l’événement dans Calendrier, où vous répondez, et Calendrier envoie votre réponse à Exchange.

> [!IMPORTANT]
> **Microsoft 365 et EWS.** Calendrier dialogue avec Exchange via Exchange Web Services (EWS), que Microsoft désactive dans Exchange Online depuis le 1er octobre 2026, tenant par tenant. L’arrêt définitif est prévu le 1er avril 2027. Apple a annoncé une mise à jour de macOS 27 qui fait passer Calendrier et Mail à Microsoft Graph, sans date pour l’instant. D’ici là, dans une organisation où EWS est désactivé, le compte Microsoft 365 cesse de se synchroniser. Calendrier garde les événements qu’il avait déjà, et Prism les affiche **sans avertir qu’ils sont périmés**. Voir *Quand ça ne fonctionne pas* ci-dessous. Un serveur Exchange sur site n’est pas concerné.

## 1. Ajouter le compte au Mac
1. Ouvrez **Réglages Système › Comptes Internet**, puis **Ajouter un compte…** et **Microsoft Exchange**.
2. Saisissez votre nom et votre adresse e-mail professionnelle, puis **Se connecter**. La page de connexion de Microsoft s’ouvre : votre mot de passe, puis le second facteur que demande votre organisation (Authenticator, SMS…).
   - Si le Mac ne trouve pas le serveur, choisissez **Configurer manuellement** et indiquez l’adresse du serveur : `outlook.office365.com` pour Microsoft 365, ou celle que vous donne votre DSI pour un serveur sur site.
3. Dans la liste des apps, cochez **Calendriers**. Mail, Contacts, Rappels et Notes sont à votre choix, Prism ne les utilise pas. Puis **OK**.

Ouvrez **Calendrier** : le compte apparaît dans la barre latérale avec ses calendriers, en général un seul nommé **Calendrier** (ou **Calendar**, dans la langue de la boîte aux lettres). La première synchronisation peut prendre une minute.

> [!NOTE]
> Un compte déjà ajouté pour Mail seulement : dans **Réglages Système › Comptes Internet**, cliquez dessus et activez **Calendriers**.

## 2. Laisser Prism lire le Calendrier
1. Dans [Réglages › Travail › Calendrier](prism:settings/calendar), activez **Agenda** et laissez **Fournisseur** sur `macos`, la valeur par défaut.
2. **Enregistrer dans config.toml** (`⌘S`), puis **Redémarrer Prism** si vous avez modifié quelque chose : l’agenda est lu au lancement.
3. macOS demande si **Prism** peut accéder à vos calendriers : **Autoriser l’accès complet**. Prism a besoin de l’accès complet pour *lire* les événements. Un accès limité à l’ajout d’événements ne lui permet de rien lire.

Ajoutez ensuite le bloc **Aujourd’hui** à une page avec **Modifier la disposition › + Bloc**.

> [!NOTE]
> L’accès est accordé à l’app compilée (`Prism.app`). Lancé avec `pnpm dev`, macOS le demande pour le terminal qui l’a lancé, si bien que Prism n’apparaît pas dans la liste Calendriers. L’app est signée ad hoc, macOS peut donc redemander après une recompilation.

## 3. Choisir les calendriers
**Réglages › Travail › Calendrier › Calendriers** liste tous les calendriers du Mac, de tous les comptes : Exchange, iCloud, Google, abonnements, Anniversaires, Jours fériés. Laissez-les tous décochés et Prism les lit tous. Cochez-en certains et il ne lit que ceux-là. Décocher un calendrier dans la barre latérale de Calendrier ne fait que le masquer dans Calendrier : Prism le lit toujours, choisissez donc ici.

Les calendriers sont choisis **par leur nom**, sans tenir compte de la casse. Deux comptes ont souvent chacun un calendrier nommé **Calendrier** (Exchange et iCloud, par exemple) : cocher ce nom lit les deux. Pour ne garder que celui d’Exchange, renommez l’autre dans Calendrier (**Lire les informations**, sur le calendrier dans la barre latérale), puis cochez celui d’Exchange par son nom.

Les calendriers partagés et délégués (celui d’un manager, d’une salle de réunion) apparaissent dès que Calendrier les affiche. Ajoutez une délégation dans **Calendrier › Réglages › Comptes**, le compte Exchange, **Délégation**.

## Fraîcheur
Calendrier se synchronise avec Exchange de lui-même, selon **Calendrier › Réglages › Comptes › Actualiser les calendriers** (**Push**, ou toutes les 5, 15, 30 minutes…), et Prism lit le résultat chaque minute. Une réunion ajoutée dans Outlook apparaît donc après la synchronisation de Calendrier, puis dans la minute. Pour forcer une synchronisation, ouvrez Calendrier et appuyez sur `⌘R`.

Le rappel 5 minutes avant une réunion, le compte à rebours et le bouton **Rejoindre** suivent ce que sait Calendrier : une invitation que Calendrier n’a pas encore synchronisée ne déclenche rien.

## Quand ça ne fonctionne pas
- **Le bloc Aujourd’hui demande l’accès**, ou Prism est absent de **Réglages Système › Confidentialité et sécurité › Calendriers** : l’accès a été refusé, ou l’app a été recompilée. Activez **Prism** dans cette liste, avec l’accès complet. S’il n’y figure pas, lancez `tccutil reset Calendar app.prism.desktop` dans un terminal, puis redémarrez Prism pour que macOS redemande.
- **Aucune réunion Exchange, les autres sont là** : dans Calendrier, vérifiez que les calendriers du compte sont listés. S’ils ne le sont pas, activez **Calendriers** pour le compte (étape 1). S’ils le sont, vérifiez que **Calendriers** dans les Réglages de Prism ne les exclut pas.
- **Les réunions ne changent plus** (une réunion déplacée ou annulée dans Outlook s’affiche toujours comme avant, les nouvelles n’apparaissent jamais), ou Calendrier marque le compte d’un ⚠ ou redemande sans cesse le mot de passe : s’il s’agit d’un compte Microsoft 365, EWS est probablement désactivé dans votre organisation (voir l’avertissement ci-dessus). Vérifiez dans Outlook sur le web que la réunion a vraiment changé. Ensuite :
  - demandez à votre DSI si EWS est encore autorisé pour Mail et Calendrier d’Apple, Microsoft leur permettant de l’autoriser jusqu’en avril 2027 ;
  - ou installez la mise à jour de macOS qui fait passer Calendrier à Microsoft Graph, une fois qu’Apple la publie ;
  - en attendant, Outlook pour Mac continue de fonctionner, mais son calendrier ne fait pas partie du Calendrier de macOS, Prism ne le voit donc pas.
- **« Votre organisation exige… »**, ou la connexion est refusée : votre organisation bloque peut-être les apps natives du Mac (accès conditionnel). Demandez à votre DSI.
- **Pas de bouton Rejoindre** : le lien de visio n’est ni dans le lien de l’événement, ni dans son lieu, ni dans le texte de son invitation, ou ce n’est pas un lien Teams, Zoom ou Meet. Ouvrez l’événement et le texte de son invitation pour vérifier.
- **Une réunion apparaît deux fois** : la même invitation est dans deux calendriers lus (le vôtre et un calendrier délégué, par exemple). Décochez l’un d’eux dans **Calendriers**.

[Ouvrir Réglages › Travail › Calendrier](prism:settings/calendar)
[Ouvrir Réglages › Travail › Calendrier › Calendrier macOS](prism:settings/calendar-macos)
