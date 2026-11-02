---
title: Se connecter à Google (Gmail, Google Agenda)
group: Start
related: setup, notes, settings
---
Le bloc Mail peut lire votre boîte **Gmail**, et le bloc Aujourd’hui votre **Google Agenda**, sur tout système : Linux et Windows compris, où il n’y a pas de Calendrier macOS. Les deux se connectent avec un compte Google par profil, via un client OAuth qui vous est propre, créé une fois dans Google Cloud en une dizaine de minutes. Rien n’est partagé avec personne : le client est à vous, la connexion aussi.

## Ce que Prism lit
Prism ne demande que les autorisations des sources que vous avez activées, toutes en lecture seule :

| Source | Autorisation | Ce que Prism lit |
|---|---|---|
| Gmail (bloc Mail) | `gmail.metadata` | Les libellés et les en-têtes de vos messages : l’expéditeur, l’objet, la date, non lu et suivi. Jamais un corps, un extrait ni une pièce jointe. |
| Google Agenda (bloc Aujourd’hui) | `calendar.calendarlist.readonly`, `calendar.events.readonly` | La liste de vos agendas, et leurs événements. Prism ne garde que ce que le bloc montre ; la description d’un événement est lue quand vous l’ouvrez, et jamais conservée. |

Prism n’envoie, ne supprime, n’archive, ne marque comme lu et ne répond jamais à rien : il ne le peut pas. Un message s’ouvre dans Gmail sur le web ; on répond à une invitation dans Google Agenda sur le web.

## 1. Créer un projet Google Cloud
Ouvrez [console.cloud.google.com](https://console.cloud.google.com) avec le compte Google que vous voulez lire, un compte personnel convient. Dans le sélecteur de projet en haut, **Nouveau projet**, nommez-le `Prism`, **Créer**, puis sélectionnez-le. Google Cloud peut vous demander d’accepter ses conditions : aucun compte de facturation n’est nécessaire, ces API sont gratuites à cet usage.

## 2. Activer les API
Dans **API et services › Bibliothèque**, cherchez et **Activez** celles que vous utiliserez :

- **Gmail API**, pour le bloc Mail ;
- **Google Calendar API**, pour le bloc Aujourd’hui.

## 3. Configurer l’écran de consentement
Dans **API et services › Écran de consentement OAuth** (la console peut l’appeler **Google Auth Platform**), **Commencer** :

1. **Informations sur l’application** : le nom de l’app, `Prism`, et votre e-mail comme e-mail d’assistance.
2. **Cible** : **Externe**. Un compte Gmail personnel n’a pas d’autre choix. Avec un compte Google Workspace (celui de votre entreprise), **Interne** est préférable si votre administrateur le permet : pas d’avertissement à la connexion, et pas d’expiration.
3. **Coordonnées** : votre e-mail. Acceptez le règlement, **Créer**.
4. **Accès aux données › Ajouter ou supprimer des champs d’application** : ajoutez `.../auth/gmail.metadata`, `.../auth/calendar.calendarlist.readonly` et `.../auth/calendar.events.readonly` (ceux que vous utilisez), **Mettre à jour**, **Enregistrer**. Google les affiche alors sur sa page de consentement.
5. **Branding** : complétez la page au-delà du nom et de l’e-mail d’assistance, sinon Google peut refuser de publier l’app :
   - **Logo de l’application** : facultatif. Laissez-le de côté sauf besoin : Google peut alors exiger une validation de la marque avant de l’afficher.
   - **Domaine de l’application** : la **Page d’accueil de l’application**, le **Lien vers les règles de confidentialité** et le **Lien vers les conditions d’utilisation**. Pour votre propre usage, n’importe quelles pages que vous contrôlez conviennent : votre fork de Prism ou une page de votre site. Elles s’affichent sur la page de consentement de Google, et seulement pour vous.
   - **Domaines autorisés** : le domaine de ces liens (`github.com`, `example.org`…), ajouté automatiquement ou via **Ajouter un domaine**.
   - **Coordonnées du développeur** : votre e-mail.

   Puis **Enregistrer**.
6. **Cible › État de publication** : **Publier l’application**, puis **Confirmer** : l’app passe **En production**.

> [!IMPORTANT]
> Ne laissez pas l’app **En test**. Google fait alors expirer votre connexion au bout de 7 jours, et Prism vous déconnecterait chaque semaine. En production sans validation, l’app continue de fonctionner pour vous : Google affiche un avertissement à la connexion (voir l’étape 5 ci-dessous), et la limite à 100 utilisateurs, ce qui est plus que suffisant. Ne soumettez pas l’app à validation : c’est fait pour les apps publiées au public.

## 4. Créer le client OAuth
Dans **API et services › Identifiants** (ou **Clients** dans Google Auth Platform), **Créer des identifiants › ID client OAuth** :

- **Type d’application** : **Application de bureau**. Pas « Application Web » : un client de bureau est celui qui accepte la réponse de Prism sur votre propre ordinateur (`http://127.0.0.1`, n’importe quel port), et n’a aucune URI de redirection à configurer.
- **Nom** : `Prism`, puis **Créer**.

Google affiche l’**ID client** (`…apps.googleusercontent.com`) et le **Code secret du client** (`GOCSPX-…`). Gardez la fenêtre ouverte, ou **Télécharger le fichier JSON**.

## 5. Se connecter depuis Prism
Dans les Réglages, **Mail** et **Calendrier** listent chacun leurs fournisseurs en sous-pages : **Microsoft 365** et **Gmail** sous Mail, **Calendrier macOS** et **Google Agenda** sous Calendrier. Celui qui n’est pas choisi est grisé, marqué *inactif*. **Gmail** et **Google Agenda** montrent le même compte Google : configurez-le dans l’un ou l’autre.

1. Choisissez ce que vous voulez lire :
   - Gmail : dans [Réglages › Travail › Mail](prism:settings/mail), activez **Mail** et réglez **Fournisseur** sur `gmail` (ou **Faire de Gmail le fournisseur de mail** sur la page Gmail) ;
   - Google Agenda : dans [Réglages › Travail › Calendrier](prism:settings/calendar), activez **Agenda** et réglez **Fournisseur** sur `google`.
2. Dans [Mail › Gmail](prism:settings/mail-gmail) (ou [Calendrier › Google Agenda](prism:settings/calendar-google)), collez l’**ID client**, et le **Code secret du client** dans son champ, puis **Enregistrer dans le trousseau**.
3. **Enregistrer dans config.toml** (`⌘S`), puis **Redémarrer Prism** : les fournisseurs sont lus au lancement.
4. De retour sur cette page, **Se connecter avec Google**. Votre navigateur s’ouvre sur la page de Google :
   - choisissez le compte (celui dont le projet contient le client, ou n’importe quel compte si l’app est en production) ;
   - Google prévient **« Google n’a pas validé cette application »** : c’est votre propre app. Cliquez sur **Paramètres avancés**, puis **Accéder à Prism (non sécurisé)** ;
   - **cochez toutes les cases** que Google propose, puis **Continuer**.
5. L’onglet indique *Prism est connecté à Google* : fermez-le. Les Réglages affichent **Connecté**, et les blocs lisent aussitôt.

Ajoutez ensuite les blocs à une page avec **Modifier la disposition › + Bloc** : **Mail**, et **Aujourd’hui** pour l’agenda.

> [!NOTE]
> Prism attend la réponse de Google pendant 5 minutes, sur votre propre ordinateur : connectez-vous dans un navigateur de la machine qui fait tourner Prism.

## Les agendas lus
Une fois connecté, **Réglages › Travail › Calendrier › Calendriers** liste vos agendas Google (rouvrez les Réglages si la liste est vide). Aucun coché : les agendas cochés dans la liste de Google Agenda lui-même. Cochés : seulement ceux-là, par nom. Un agenda que vous avez renommé dans Google Agenda se lit sous le nom que vous lui avez donné.

## Où vont les secrets
Le code secret du client et le jeton d’actualisation vont dans le trousseau du système, jamais dans `config.toml` : le Trousseau de macOS, le Gestionnaire d’identification de Windows, ou Secret Service sous Linux (GNOME Keyring, KWallet). Leurs entrées sont `prism-google-secret` et `prism-google`, suivies de `-<profile>` pour un profil autre que celui par défaut. Chaque profil a son propre compte Google : collez le même client dans deux profils pour vous connecter avec deux comptes.

Sous Linux sans Secret Service (un bureau minimal, un gestionnaire de fenêtres seul), installez et démarrez d’abord `gnome-keyring`. `prism google-secret` lit aussi le code secret du client sur son entrée standard, pour le profil actif : lancez-le, collez le secret, puis `Ctrl-D`.

## Activer la seconde source plus tard
Si vous vous êtes connecté pour Gmail, puis activez Google Agenda (ou l’inverse), Prism n’a pas encore reçu la nouvelle autorisation : le bloc indique *Google n’a pas accordé l’accès à Google Agenda*. **Reconnectez-vous** dans Réglages › Travail › Mail › Gmail (ou Calendrier › Google Agenda), et cochez toutes les cases.

## Se déconnecter
**Se déconnecter** (deux clics) révoque la connexion auprès de Google et retire le jeton d’actualisation du trousseau ; le code secret du client reste. Vous pouvez aussi retirer l’accès de Prism depuis votre compte Google, sur [myaccount.google.com/permissions](https://myaccount.google.com/permissions) : Prism indique alors qu’il est déconnecté à sa lecture suivante.

## Quand ça ne fonctionne pas
- **« Accès bloqué : Prism n’a pas terminé la procédure de validation de Google »** : l’app est encore en test, et le compte ne fait pas partie de ses utilisateurs test. Publiez-la (étape 3.6), ou ajoutez le compte sous **Cible › Utilisateurs test**.
- **Déconnecté chaque semaine** (*Token has been expired or revoked*) : l’app est encore en test. Publiez-la, puis reconnectez-vous.
- **« Google n’a pas accordé l’accès à … »** : une case est restée décochée sur la page de Google, ou une source a été activée depuis. Reconnectez-vous et cochez toutes les cases.
- **« Gmail API has not been used in project … or it is disabled »** (ou Google Calendar API) : activez-la (étape 2), attendez une minute, et Prism relit de lui-même.
- **« invalid_client »** ou **« redirect_uri_mismatch »** : l’ID client ou le secret est faux, ou le client n’est pas du type **Application de bureau**. Créez un client Application de bureau et collez les deux à nouveau.
- **« no Google client secret in the keychain »** : collez-le dans Réglages › Travail › Mail › Gmail (ou Calendrier › Google Agenda).
- **Un compte d’entreprise est refusé** : votre administrateur Google Workspace bloque peut-être les apps qu’il n’a pas approuvées. Demandez-lui, ou créez le projet dans l’organisation de votre entreprise avec la cible **Interne**.
- **Rien ne se passe à la connexion** : aucun navigateur par défaut ne s’est ouvert. Définissez-en un (`xdg-settings set default-web-browser …` sous Linux), puis reconnectez-vous.

[Ouvrir Réglages › Travail › Mail › Gmail](prism:settings/mail-gmail)
[Ouvrir Réglages › Travail › Calendrier › Google Agenda](prism:settings/calendar-google)
