---
title: Notes, rappels, agenda et mail
group: Everyday work
blocks: notes, agenda, mail
related: time, todo, settings, keyboard
---
Gardez ce que vous ne devez pas oublier à côté de votre travail : notes libres, petites tâches, rappels notifiés par le système, les réunions du jour lues dans votre Calendrier, et vos mails non lus.

## Le bloc Notes
Le bloc **Notes** contient de courtes notes qui restent sur cette machine. Tapez dans le champ et appuyez sur `Enter` pour en ajouter une ; `Shift+Enter` commence une nouvelle ligne. Au-dessus du champ, trois boutons choisissent le type de note :

| Type | Affichée |
| --- | --- |
| **Épinglée** | Toujours, quel que soit le jour. |
| **Aujourd’hui** (ou le jour choisi dans l’en-tête) | Seulement quand ce jour est sélectionné dans le sélecteur de date. |
| **Rappel** | En haut, jusqu’à ce qu’il soit fait, avec une notification système à son heure. |

Le dernier type utilisé est retenu. Dans une note, les identifiants de ticket et les références de MR deviennent des liens : un clic filtre tout le tableau de bord dessus, la petite flèche les ouvre.

Pour modifier une note, double-cliquez sur son texte (ou donnez-lui le focus et appuyez sur `Enter`), modifiez, puis `Enter` pour enregistrer ou `Esc` pour annuler. Pour la supprimer, cliquez sur son ✕, puis sur **Supprimer ?** dans les secondes qui suivent. Son ➤ (affiché quand un agent tourne) liste les agents en cours, celui sélectionné dans la vue Agents en premier, chacun avec son ticket ou sa branche et s’il vous attend ou travaille : choisissez-en un, et la note lui est envoyée comme prompt, sans ses étiquettes. Tout de suite, même pendant qu’il travaille ou après une interruption : sa CLI la garde et la lit une fois son tour terminé. Seul un menu à son écran (une autorisation, sa question) la retient, envoyée d’elle-même une fois que vous y avez répondu (les notes envoyées entre-temps suivent, dans l’ordre ; la file survit à un redémarrage de Prism). Une note déjà en file pour cet agent, ou envoyée dans les 10 dernières secondes (un double clic), n’est pas envoyée deux fois. Une note TODO envoyée passe en DOING. Sur le téléphone, le même ➤ tape la note à l’agent choisi, comme le fait sa zone de réponse : avec **saisie** activée (Réglages › Accès à distance › Exposition), sa passkey une fois par agent pendant un moment. Au-delà de cinq notes, un champ de recherche apparaît.

Les notes et la mise en page sont enregistrées en JSON dans le dossier de données de l’application, `~/Library/Application Support/app.prism.desktop/`.

## Les notes comme tâches
Une note épinglée ou une note du jour peut devenir une petite tâche. Survolez son bord gauche pour voir la pastille de statut :

1. Cliquez une fois : la note devient **TODO**.
2. Cliquez à nouveau : **DOING**.
3. Encore une fois : **DONE**. Un quatrième clic retire le statut.

Vous pouvez aussi commencer le texte par `TODO` ou `DOING` (par exemple `TODO appeler l’équipe ops`) : le mot fixe le statut et disparaît de la note. Les notes en cours viennent d’abord, puis celles à faire, puis les notes simples. Le titre du bloc compte les notes en cours. Les notes terminées quittent la liste et rejoignent une section **Terminées** repliée en bas, avec les rappels accomplis.

## Étiquettes
Écrivez une étiquette dans la note elle-même : `#infra`, `#perso`, `#client/acme` (une lettre d’abord ; `#1957` reste une référence, `C#` n’est pas une étiquette). Modifiez la note pour changer ses étiquettes.
- **Filtrer** : cliquez sur une étiquette, dans une note ou dans la barre d’étiquettes sous le champ, pour ne voir que ses notes ; cliquez à nouveau (ou sur l’étiquette dans le titre du bloc) pour toutes les voir. Une note écrite pendant qu’une étiquette est filtrée reçoit cette étiquette.
- **Trier** : le bouton A→Z ordonne les notes par leur première étiquette (celles sans étiquette en dernier), puis par statut.
- **Grouper** : le bouton des calques remplace les sections Épinglées et du jour par une section par étiquette (une note à deux étiquettes apparaît dans les deux), et **Sans étiquette** en dernier. Les rappels gardent leur section.

## Les notes d’un dépôt
Une note peut concerner l’un de vos dépôts (Réglages › Dépôts), quel que soit son type. Le sélecteur **branche** du champ range la prochaine note sous un dépôt (— pour aucun) ; le bouton branche d’une note, au survol, la déplace vers un autre dépôt ou la sort de tout dépôt.

- **Filtrer** : cliquez sur le badge de dépôt d’une note, ou sur sa pastille dans la barre sous le champ, pour ne voir que les notes de ce dépôt ; cliquez à nouveau (ou sur le nom dans le titre du bloc) pour toutes les voir. Une note écrite pendant qu’un dépôt est filtré est rangée sous lui.
- **Grouper** : le menu de regroupement à côté du tri propose **Dépôt** : une section par dépôt, par nom, et **Sans dépôt** en dernier.
- **Menu Dépôts** : dans la barre d’état, chaque dépôt indique combien de notes ouvertes il a ; un clic ouvre les notes par-dessus la vue, filtrées sur lui, prêtes pour une nouvelle.
- **Agents** : le ➤ d’une note liste d’abord les agents qui travaillent dans son dépôt. Dans la vue Agents, le panneau **Notes** d’un agent liste les notes ouvertes du dépôt où il travaille (celui de son worktree, ou le clone qui contient son dossier) : **Envoyer** lui en donne une, son statut et son ✕ la modifient ou la suppriment là, et une note écrite là est rangée sous ce dépôt.

## Rappels
Choisissez **Rappel** au-dessus du champ, tapez ce dont il faut se souvenir, puis choisissez une heure : **dans 30 min**, **dans 1 h**, **14:00**, **demain 9:00**, **lundi 9:00** (seules celles encore à venir sont proposées), ou n’importe quelle date et heure dans le champ. Sans choix, le rappel est fixé une heure plus tard.

Les rappels se trouvent en haut du bloc, le plus proche d’abord, avec le temps restant (`dans 25 min`, `aujourd’hui 14:00`). Quand l’un d’eux arrive à échéance, Prism envoie une notification système, une fois ; un rappel en retard passe au rouge (`12 min de retard`) et le titre indique combien sont échus. Ensuite :

- cochez sa case pour le marquer comme fait (il passe dans **Terminées** ; décochez-le là pour le rouvrir) ;
- ou cliquez sur **Plus tard** pour le repousser : **dans 10 min**, ou l’un des mêmes préréglages. Il sera notifié à nouveau.

> [!TIP]
> Un rappel manqué pendant que Prism ne tournait pas est quand même notifié à sa prochaine vérification, comme « Rappel (manqué) ».

## Notes rapides depuis n’importe où
Appuyez sur `⌘⇧N` pour ouvrir les notes par-dessus n’importe quelle vue, Agents et Réglages compris, avec le curseur dans le champ. `Enter` enregistre et garde le champ ouvert pour la note suivante ; `Esc` ferme. Changez le raccourci dans Réglages › Travail › Notes (`[notes] key`). S’il contient `⌘` ou `⌃`, le raccourci fonctionne aussi pendant que vous tapez dans un champ.

[Ouvrir Réglages › Travail › Notes](prism:settings/notes)

## Le bloc Aujourd’hui : votre agenda
![Le bloc Aujourd’hui](images/agenda.webp){width=563}


Le bloc **Aujourd’hui** montre les réunions du jour, lues localement dans le Calendrier de macOS : un compte Exchange, Microsoft 365 ou Google ajouté dans Réglages Système → Comptes Internet en fait partie, et aucun jeton ni appel réseau n’est nécessaire (un compte Exchange, pas à pas : [Exchange dans le Calendrier de macOS](prism:help/exchange)). Sur Linux ou Windows, ou pour lire Google Agenda directement, réglez **Fournisseur** sur `google` : le bloc lit alors l’API Google Calendar, en lecture seule, connecté avec votre compte Google ([Se connecter à Google](prism:help/google)).

- Le titre compte vos réunions et leur durée totale, puis le temps restant aujourd’hui.
- La réunion en cours est mise en évidence avec sa progression ; sinon vous voyez le compte à rebours jusqu’à la suivante.
- **Rejoindre** ouvre le lien Teams, Zoom ou Meet ; il s’allume 15 minutes avant le début.
- Les réunions refusées, annulées ou « disponibles » sont barrées.
- Un clic sur une réunion ouvre son détail : les participants et leurs réponses, et le texte de l’invitation. **Répondre dans Calendrier** l’ouvre dans Calendrier (**Répondre dans Google Agenda**, sur le web, avec Google), puisque Prism ne peut pas répondre lui-même à une invitation. **Préparer** confie la réunion à un agent, en lecture seule (en mode plan, puisque n’importe qui peut vous envoyer une invitation) : son sujet, les participants et leur lien avec vos tickets, MR et notes, les points à soulever.

Le bloc suit le sélecteur de date de l’en-tête : choisissez un autre jour pour voir son agenda.

Prism envoie aussi une notification avant chaque réunion où vous êtes attendu, même quand sa fenêtre est fermée. Le délai est de 5 minutes par défaut : `[alerts] meeting_lead_minutes` (Réglages › Interface › Alertes et son).

> [!NOTE]
> Avec le Calendrier de macOS, au premier lancement, macOS demande si Prism peut lire vos calendriers. Si vous avez refusé, autorisez Prism dans Réglages Système → Confidentialité et sécurité → Calendriers, puis relancez Prism.

## Réglages du calendrier
Dans Réglages › Travail › Calendrier :

- **Agenda** (`[calendar] enabled`) : désactivé, Prism ne lit jamais de calendrier et ne demande pas d’accès. Il n’y a alors ni bloc Aujourd’hui, ni compte à rebours, ni notification de réunion. Le changement s’applique au prochain lancement.
- **Fournisseur** (`[calendar] provider`) : `macos` (par défaut, macOS uniquement) ou `google` (tout système). Lu au lancement. Chacun a sa page sous Calendrier : **Calendrier macOS**, et **Google Agenda** avec le compte Google auquel se connecter ; le fournisseur non choisi est grisé, et sa page propose de devenir le calendrier.
- **Calendriers** (`[calendar] calendars`) : les calendriers lus, cochés parmi ceux du fournisseur ; aucun coché, tous ceux du Calendrier de macOS (anniversaires et jours fériés compris), ou ceux cochés dans Google Agenda. Un calendrier coché puis renommé ou supprimé reste dans la liste, barré, à décocher.

[Ouvrir Réglages › Travail › Calendrier](prism:settings/calendar)
[Ouvrir le bloc Aujourd’hui](prism:block/agenda)

## Le bloc Mail
![Le bloc Mail : les non lus d’abord, les suivis marqués](images/mail.webp)

Le bloc **Mail** liste les messages non lus de votre boîte Microsoft 365 ou Gmail, du plus récent au plus ancien, et ceux suivis (étoilés), lus ou non : l’expéditeur, l’objet et depuis combien de temps. Un clic ouvre le message dans Outlook ou Gmail sur le web ; une fois lu là-bas, il quitte la liste à la lecture suivante, en moins d’une minute.

Avec **Fournisseur** réglé sur `gmail` (`[mail] provider`), il lit Gmail par l’API Gmail avec `gmail.metadata` : les libellés et les en-têtes seulement, jamais un corps, un extrait ni une pièce jointe. Il se connecte avec votre compte Google, dans Réglages › Travail › Mail › Gmail : voir [Se connecter à Google](prism:help/google). Le reste de cette section concerne Microsoft 365.

Il lit votre boîte par **Microsoft Graph**, avec la permission la plus restreinte qui existe, **Mail.ReadBasic**, déléguée : votre propre boîte seulement, et jamais le corps d’un message, son aperçu ni ses pièces jointes. Il faut une application enregistrée dans Entra ID par votre organisation (son **ID d’application (client)**), généralement fournie par le pack de votre équipe.

Pour le configurer : activez **Réglages › Travail › Mail** avec le fournisseur `microsoft`, vérifiez l’application dans **Mail › Microsoft 365**, redémarrez Prism, puis **Se connecter** là : Prism affiche un code et ouvre la page de Microsoft, où vous le tapez, vous connectez avec votre compte professionnel et acceptez. Le jeton de rafraîchissement va dans le trousseau du système, dans l’entrée de ce profil, jamais dans `config.toml` ; **Se déconnecter** le supprime. Ajoutez ensuite le bloc à une page avec **+ Bloc**.

- **Application** (`[mail] client_id`) et **Tenant** (`[mail] tenant`, vide : n’importe quel compte professionnel).
- **Fournisseur** (`[mail] provider`) : `microsoft` (par défaut) ou `gmail`. Chacun a sa page sous Mail, **Microsoft 365** (l’application, le tenant et la connexion) et **Gmail** (le compte Google) ; le fournisseur non choisi est grisé.
- **Suivis** (`[mail] flagged`) : les messages suivis aussi, les étoilés dans Gmail (activé par défaut).
- **Messages** (`[mail] max`) : listés au plus, 30 par défaut.

[Ouvrir Réglages › Travail › Mail](prism:settings/mail)
[Ouvrir le bloc Mail](prism:block/mail)
