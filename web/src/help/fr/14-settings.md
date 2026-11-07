---
title: Réglages et config.toml
group: Make it yours
related: setup, keyboard, fun, ref-settings
---
Chaque réglage de Prism vit dans un seul fichier, `~/.config/prism/config.toml`, et la vue **Réglages** le reflète champ par champ. Changez une valeur à l’un ou l’autre endroit : c’est le même réglage.

## Ouvrir les Réglages
![Réglages : l’arbre des domaines à gauche, la section ouverte à droite](images/settings.webp)


Appuyez sur `⌘,`. `Esc` ferme la vue.

La colonne de gauche est un arbre de pages, rangées selon ce qu’elles configurent :
- **Général** : Profils, Packs, Fichier de config, Logs, Dépendances, Mises à jour, Retours.
- **Interface** : Apparence, Pages, Alertes et son, Ouvrir avec.
- **Travail** : Tracker (avec ClickUp, Jira et Obsidian en dessous), Calendrier, Mail, Notes.
- **Code** : Forge (avec GitLab et GitHub en dessous), Dépôts.
- **CI** : Renovate, et les pages des blocs d’une équipe, s’il y en a.
- **Sécurité** : Vulnérabilités.
- **Agents** : Agent (avec Claude Code, Command Code, OpenCode et Codex en dessous), Console.
- **Infra** : Kubernetes (avec Pods et ArgoCD en dessous), Docker, Data (GCP), et les pages des blocs d’une équipe, s’il y en a.
- **Accès à distance** : Accès, Appareils, Exposition, Sessions, Absence (Prism depuis un téléphone), et Machines (le Prism d’un autre ordinateur, et ce que les autres ordinateurs peuvent faire ici).

Partout où il y a plusieurs fournisseurs, la page de la fonctionnalité contient ce qu’ils partagent et le choix, et chaque fournisseur a sa propre page avec uniquement ses clés : Forge contient `[forge]`, Forge › GitLab `[gitlab]` (hôte, label de la file de merge) ; Agent contient `[agents]`, et chaque CLI son `[agents.<cli>]` (**Binaire** et **Arguments supplémentaires**, ajoutés à chaque lancement et reprise ; Claude Code aussi ses sessions Claude Desktop, l’alerte de coût et le jeton d’usage du forfait). Une page peut regrouper plusieurs tables du fichier : Dépôts contient `[[workspaces]]`, `[[repos]]` et `[worktrees]`, et une page faite de plusieurs étapes peut les montrer en onglets. Les tables d’une page sont listées à côté de son titre, et chaque partie de la page nomme la sienne. Le chevron replie une branche ; quand l’arbre a le focus, `←` et `→` replient et déplient la page courante.

Une fonctionnalité qui peut être désactivée (Docker, Vulnérabilités, ClickUp…) a son interrupteur dans l’en-tête de la page, et un point dans l’arbre : plein quand elle est activée, creux quand elle est désactivée. Un fournisseur autre que celui choisi affiche **inutilisé** (une CLI d’agent qu’un dépôt utilise, **N dépôts**) ; sa page reste modifiable, ses clés s’appliquent une fois qu’il est choisi.

Le champ **Rechercher un réglage ou une clé…** filtre l’arbre par nom de page, table (`[gitlab]`, `[[repos]]`) ou clé (`merge_queue_label`). **Activés**, **Modifiés** et **À vérifier** ne montrent que les fonctionnalités activées, les pages avec des modifications non enregistrées, ou celles où une vérification a échoué.

[Ouvrir les Réglages](prism:settings/appearance)

## Lire un champ
Chaque champ montre son libellé, une courte explication et sa clé dans le fichier. Deux badges indiquent quand une modification prend effet :
- **immédiat** : appliqué dès l’enregistrement.
- **redémarrage** : lu au lancement, appliqué au prochain démarrage de Prism.

Un dossier (le clone d’un dépôt, le dossier des worktrees, le coffre Obsidian…) peut être tapé, ou choisi avec le bouton dossier à côté, qui ouvre le sélecteur du système ; un dossier de votre dossier personnel s’écrit `~/…`. Sous **Packs**, **Choisir…** sélectionne de la même façon un fichier de pack à importer.

À droite, **par défaut** signifie que la clé n’est pas dans votre fichier et que la valeur intégrée s’applique. Une fois réglée, **Rétablir la valeur par défaut** retire à nouveau la clé. Un champ modifié est mis en évidence jusqu’à l’enregistrement.

Certaines valeurs sont vérifiées en direct : le projet d’un dépôt sur la forge, un contexte kube, un chemin de dépôt, un dossier, votre connexion gcloud. Le verdict apparaît à côté du champ, avec une coche ou un avertissement, et **Vérifier à nouveau** relance les vérifications. Pour une source (la forge, ClickUp, Worktrees…), le haut de la section indique aussi si elle a été **lue correctement avec la config actuelle** ou pourquoi la dernière lecture a échoué.

> [!TIP]
> Une icône d’avertissement à côté d’une page de l’arbre signifie qu’une vérification y a échoué, ou que cette source n’a pas pu être lue. Un point signale des modifications non enregistrées.

## Enregistrer
Les modifications ne sont écrites qu’à l’enregistrement.

1. Modifiez autant de champs que vous voulez, dans n’importe quelle section. Une barre en bas compte les modifications non enregistrées.
2. Appuyez sur `⌘S` ou cliquez sur **Enregistrer dans config.toml**. **Annuler** les abandonne.
3. Si un réglage enregistré est lu au lancement, la barre nomme les sections concernées et propose **Redémarrer Prism**.

Après avoir modifié `config.toml` à la main, `restart` dans la console, ou `prism restart` dans un terminal, redémarre Prism pour qu’il le relise.

Le bouton **TOML** montre le fichier à côté des réglages, avec ce que l’enregistrement écrirait. L’enregistrement ne réécrit que les clés modifiées : vos commentaires, l’ordre et la mise en forme du fichier sont conservés, et la version précédente est copiée dans `config.toml.bak`.

Avec des packs, un menu en haut de ce panneau passe d’une vue à l’autre parmi trois :
- **config.toml** : votre fichier, avec ce que l’enregistrement écrirait.
- **Pack <nom>** : le fichier du pack tel qu’importé dans `packs/`, en lecture seule (**Mettre à jour** dans Réglages › Général › Packs le relit).
- **Fusionné** : les packs, puis votre fichier par-dessus, tels que Prism les combine : les tables clé par clé, `[[repos]]` par nom, le reste remplacé ; l’en-tête `[pack]` du pack et les sections qu’un pack ne peut pas définir sont laissés de côté. Les modifications non enregistrées y apparaissent, mises en évidence. Les valeurs par défaut intégrées ne sont pas affichées.

> [!NOTE]
> Les réglages qui s’appliquent à l’enregistrement comprennent Apparence, Console, Notes, Alertes, Pages, Agents, le délai d’obsolescence des worktrees et les statuts du workflow ClickUp. La plupart des réglages de sources (la forge, les listes ClickUp, Runners, Data…) attendent un redémarrage.

## Depuis le téléphone
Avec **Réglages** activé dans Réglages › Accès à distance › Exposition (`[remote] settings`, désactivé par défaut), l’accès à distance a **Plus › Réglages** : le même arbre, chaque valeur lue, jamais les jetons (ils restent dans le trousseau). Quelques clés se modifient de là, marquées *modifiable* : les notifications (digest, heures du digest, ce qui notifie encore tout de suite, heures calmes, événements de concentration), les seuils d’alerte, et l’apparence (thème, langue, densité, animations, infobulles, économiseur d’écran). **Enregistrer** montre ce qui va changer, puis demande la passkey du téléphone, à chaque fois ; l’ordinateur vérifie à nouveau chaque clé avant d’écrire config.toml, et journalise quel téléphone a changé quoi. Tout ce qui lance un programme, laisse entrer quelqu’un ou décide de ce que Prism lit (agents, serveurs de dev, mises à jour, packs, profils, l’accès à distance lui-même) se change uniquement sur l’ordinateur.

**Nouveaux projets depuis le téléphone.** Ajoutez vos **Dossiers de travail** dans Réglages › Code › Dépôts (`[[workspaces]]`), un par type de projet — par exemple *Travail* dans `~/Development` avec le groupe de votre entreprise, *Perso* dans `~/Perso` sans groupe (votre propre compte) et avec sa propre **Forge**, GitHub alors que le profil est sur GitLab. Les projets d’un dossier de travail sont clonés depuis sa forge, créés et publiés dessus (glab ou gh, connectés sur l’ordinateur) ; un projet sur une autre forge que celle du profil s’écrit avec `forge = "github"` dans `[[repos]]` : le menu Dépôts y renvoie, et ses MR rejoignent le même bloc, lues avec l’outil et la connexion de cette forge ; ses pipelines ne sont pas suivis. Nouveau projet et Cloner un projet les proposent sur l’ordinateur (le premier par défaut ; un clic sur l’un règle le dossier, le groupe et la forge), et l’accès à distance a **Plus › Nouveau projet** (`[remote] projects`, activé par défaut, Réglages › Accès à distance › Exposition). Le téléphone choisit l’un des dossiers de travail et donne le nom, son groupe pré-rempli : l’ordinateur crée le dossier là et nulle part ailleurs, avec un dépôt git sur la branche principale, un README.md et un premier commit, et sur demande le projet sur GitLab ou GitHub (glab ou gh, la connexion de l’ordinateur), défini comme origin et poussé. Votre passkey est demandée à chaque fois, et l’ordinateur journalise quel téléphone a créé quoi. Le projet est ajouté à `[[repos]]` dans config.toml, avec les worktrees activés ; il apparaît dans les dépôts une fois Prism redémarré (**Plus › Redémarrer Prism**). Un projet créé sans la forge reste local aussi longtemps que vous voulez : **Gardés en local**, sous le formulaire, liste les dépôts qui ont un clone et pas de projet (celui tout juste créé compris), et **Publier** crée son projet sur la forge (le groupe de son dossier de travail proposé), au nom de son dossier, le définit comme origin et pousse sa branche courante, avec votre passkey ; son `project` est écrit dans config.toml. Sur l’ordinateur, **Publier sur GitLab…** (ou GitHub) dans Réglages › Code › Dépôts, le bouton **Publier…** d’un dépôt sans projet dans le menu Dépôts, ou *Publier un projet local* dans la palette font la même chose. Un dépôt qui a déjà un remote n’est pas publié : donnez plutôt son Projet. Sans dossier de travail, le téléphone ne crée rien. [Ouvrir Réglages › Code › Dépôts](prism:settings/repos)

**Plusieurs profils, un téléphone.** Chaque profil est son propre Prism, même quand tous s’affichent dans les fenêtres d’une seule app ; le téléphone les atteint tous par l’un d’eux. Le profil dont l’accès à distance a le tunnel (tailscale ou other) est le relais : le téléphone est appairé avec lui, se déverrouille chez lui, et ses notifications viennent de lui. Dans chaque autre profil, réglez Réglages › Accès à distance › Accès › **Tunnel** sur **relay** (`[remote] tunnel = "relay"`, `enabled = true`) : son Prism n’écoute que sur 127.0.0.1, sur un port libre qu’il indique au relais avec un secret (`remote/endpoint.json` dans son dossier de données, lisible par vous seul), et le téléphone l’atteint à `/p/<profil>/` de l’adresse du relais. Le relais vérifie la session et votre passkey ; chaque profil garde ses propres droits (Exposition : actions, critiques, saisie…). Sur le téléphone, l’en-tête reçoit le sélecteur de profil : **Tous** montre sur l’Accueil les agents en attente de tous les profils, chacun avec le badge de son profil et ses réponses rapides, une carte par profil, et Agents groupés par profil ; les autres vues sont celles du relais. Un profil choisi s’affiche seul, ses propres onglets, la ligne de l’en-tête et l’onglet affiché à sa couleur ; un nouvel agent y démarre. Un profil fermé a **Ouvrir** : son Prism démarre sur l’ordinateur à côté de celui du relais, avec votre passkey. Un agent en attente dans n’importe quel profil réveille le téléphone, la notification nommant le profil et l’agent, et un toucher l’ouvre là. Le Prism du relais fermé, aucun profil n’est joignable depuis le téléphone.

## Depuis un autre ordinateur
Le Prism d’un autre ordinateur — un serveur qui fait tourner `prism-server`, un second poste sous macOS ou Linux — s’ouvre dans une fenêtre de celui-ci, comme s’il tournait ici : ses pages, ses agents et leurs terminaux, ses worktrees, ses réglages, servis par ce Prism. Son accès à distance est activé (Réglages › Accès à distance › Accès, ou `[remote] enabled = true` dans son config.toml), derrière Tailscale ou au bout d’un tunnel SSH. Ajoutez-le dans Réglages › Accès à distance › Machines (`[[machines]]`) : un **Nom**, son **Adresse** (`https://build-srv.tail1234.ts.net`, ou `http://127.0.0.1:9000` avec `ssh -L 9000:127.0.0.1:<son port> build-srv` ; le HTTP en clair seulement vers cet ordinateur, pour que le jeton ne traverse jamais un réseau en clair), et au besoin son **Hôte SSH** (son nom dans `~/.ssh/config`), avec lequel ses dossiers s’ouvrent ici dans VS Code par Remote-SSH. [Ouvrir Réglages › Accès à distance › Machines](prism:settings/machines)

**Appairage.** Le menu des profils (le badge au pied du rail) le liste sous **Autres ordinateurs**. La première fois, sa fenêtre demande un code : sur cet ordinateur, lancez `prism-server peer pair` (ou Réglages › Accès à distance › Appareils › **Appairer un ordinateur**, sur un Prism avec une fenêtre). Il affiche six chiffres pendant cinq minutes ; tapez-les ici, et cet ordinateur demande d’accepter celui-ci par son nom. Une fois accepté, cet ordinateur reçoit son jeton une seule fois et le garde dans son trousseau, jamais dans la page. Cinq codes faux mettent fin à l’appairage ; un second ordinateur au nom déjà appairé reçoit le sien (`MacBook (2)`). `prism-server peer add <nom>` affiche un jeton à coller à la place (**Coller un jeton à la place**), ou à mettre dans `PRISM_MACHINE_TOKEN_<NOM>` là où aucun trousseau n’est déverrouillé ; `peer list` les liste, `peer remove <nom>` en révoque un (ou **Révoquer** dans Appareils), et ses fenêtres se ferment aussitôt.

**Sa fenêtre.** Un bandeau bleu au-dessus de la page et un liseré bleu sur le rail disent de qui est ce Prism : son nom, son profil et, injoignable, pourquoi (son Prism fermé, le tunnel coupé, pas de réseau) et depuis quand, la page atténuée montrant son dernier état jusqu’à ce qu’il réponde de nouveau. Ce qui appartient à cet ordinateur reste ici : les sélecteurs de fichiers, Afficher dans le Finder, Terminal, le Dock, les mises à jour de Prism, la connexion Google. Les fichiers du dossier d’un agent téléchargés (onglet Fichiers) sont préparés là-bas, un zip pour plusieurs, et enregistrés ici là où le dialogue de cet ordinateur l’indique. Une machine à plusieurs profils (chacun son `prism-server --profile <id> serve`, les autres avec `[remote] tunnel = "relay"` derrière celui qui a le tunnel) les liste sous son nom, chacun dans sa propre fenêtre ; un profil fermé est grisé.

**Ce qu’il peut faire, et ce qui vous demande d’abord.** Chaque machine décide, dans ses propres Réglages › Accès à distance › Machines (`[remote.peers]`) : **Actions**, **Actions critiques**, **Saisie** (terminaux, un texte à un agent, un fichier écrit) et **Réglages** (enregistrer ses réglages, packs, jetons), tous activés par défaut ; il lit toujours, et chaque appel qui modifie quelque chose y est journalisé avec le nom de cet ordinateur. Ce que cette machine classe critique — un merge, un job manuel, un agent arrêté, un worktree supprimé, un secret du dossier d’un agent téléchargé, une commande d’un de ses propres blocs — demande d’abord à la personne d’ici : Touch ID ou le mot de passe du compte sur un Mac, l’agent polkit du bureau sous Linux (refusé sans polkit). C’est l’app qui demande, pas la page ; **Sans confirmation** dans Réglages › Accès à distance › Machines s’en passe pour une machine. Cette confirmation protège de quelqu’un devant cet ordinateur, pas de son jeton entre d’autres mains : ce qui tient sur cette machine, ce sont ses propres interrupteurs. **Actions** comprend le lancement d’un agent, qui exécute ses propres commandes : activé, couper Saisie n’empêche pas cet ordinateur d’y exécuter du code. Un jeton vaut le compte de cette machine : gardez-le comme une clé SSH.

**Sans l’ouvrir.** La barre d’état a une pastille par machine : ses agents, ceux qui vous attendent (de tous ses profils), estompée quand elle est injoignable ; un clic ouvre sa fenêtre. Un agent qui y attend une autorisation ou son tour notifie ici comme l’un des vôtres, heures calmes et digest compris, et compte dans la barre des menus.

## La page Fichier de config
**Réglages › Général › Fichier de config** montre le chemin du fichier, avec **Afficher dans le Finder**, **Ouvrir dans l’éditeur** et **Recharger** (relire le fichier après l’avoir modifié à la main). Vous pouvez toujours modifier `config.toml` directement : c’est la source de vérité. Si le fichier ne se charge pas, Prism s’ouvre quand même, et l’erreur s’affiche dans les **Alertes** et dans les Réglages.

[Ouvrir Réglages › Général › Fichier de config](prism:settings/file)

## Les logs, pour dépanner
Quand une source ne se lit pas et que son message ne suffit pas, regardez les logs. Prism les écrit dans un fichier du profil, en plus de la sortie d’erreur standard : `~/Library/Logs/app.prism.desktop/prism.log` sur macOS (un profil autre que celui par défaut dans `profiles/<id>` à l’intérieur), `~/.local/state/app.prism.desktop/` sur Linux. **Réglages › Général › Logs** montre le chemin, avec **Afficher dans le Finder**, **Ouvrir**, **Copier les 300 dernières lignes** (à coller dans un signalement) et **Afficher les dernières lignes**. **Niveau** (`[logs] level`, lu au lancement) vaut `info` par défaut : réglez `debug`, redémarrez, reproduisez le problème, copiez les lignes, puis remettez-le. Le fichier ne contient jamais de jeton ni le contenu d’un message ; il tourne à 5 Mo.

[Ouvrir Réglages › Général › Logs](prism:settings/logs)

## Problèmes
Ce que Prism n’a pas pu faire ne reste plus dans le log : **Problèmes**, à droite de la barre d’état, compte ce qui échoue maintenant et ce qui a échoué depuis votre dernière ouverture, en rouge quand l’un d’eux est une erreur. Un clic les liste : **En échec maintenant** (une source qui ne se lit pas, la config ou un pack qui ne se charge pas, la source d’un pack ou des mises à jour injoignable), chacun avec **Réglages** vers l’endroit où il se corrige, puis **Récemment** (les derniers avertissements et erreurs du cœur, un problème répété compté, et les actions qui ont échoué, avec leur âge). **Copier** les met tous dans le presse-papiers, pour une issue ou un message ; **Effacer** oublie ce qui a échoué récemment (ce qui échoue maintenant reste) ; **Log** ouvre Réglages › Général › Logs. *Problèmes* dans la palette ouvre aussi la liste. Elle reste sur l’ordinateur : le téléphone ne la voit jamais.

## Dépendances
**Réglages › Général › Dépendances** liste les outils que Prism lance — `tmux`, `git`, les CLI d’agent (`claude`, `cmd`, `opencode`, `codex`, chacun à son **Exécutable** s’il est défini), `glab`, `gh`, `kubectl`, `gcloud`, `trivy`, `docker`, et sous Linux les assistants audio (`pactl`, `playerctl`) — avec la version que donne chacun, où il a été trouvé dans le PATH, et ce pour quoi Prism le lance. Un outil dont votre config a besoin (tmux et git, le CLI d’agent et celui de la forge choisis, kubectl avec Kubernetes activé, trivy avec Vulnérabilités activé) et qui manque affiche **Introuvable**, avec comment l’installer ; les autres, **Facultatif**. **Copier** met la liste dans le presse-papiers pour un signalement ; **Rafraîchir** redemande sa version à chaque outil (la réponse est sinon gardée 10 minutes, après en avoir installé ou mis à jour un). Les versions partent aussi avec une demande de retour (la ligne **Tools**), jamais les chemins.

[Ouvrir Réglages › Général › Dépendances](prism:settings/dependencies)

## Mises à jour
**Réglages › Général › Mises à jour** montre la version de ce Prism et la release la plus récente de sa source (`[updates] source` : `glab:group/project` ou `gh:owner/repo`, lue avec glab ou gh). Le pack de votre équipe la définit en général : ses builds embarquent les blocs de l’équipe, le Prism open source a la sienne. Prism vérifie au lancement et toutes les 6 heures (**Rechercher des mises à jour**) ; une version plus récente apparaît dans la barre d’état. **Installer** (deux clics) remplace Prism et le redémarre sur macOS et depuis une AppImage ; avec un `.deb` ou un paquet Arch, il télécharge le paquet pour `sudo apt install` ou `sudo pacman -U`.

[Ouvrir Réglages › Général › Mises à jour](prism:settings/updates)

## Retours
**Envoyer un retour** ouvre une issue sur le dépôt d’où vient Prism (`[feedback] repo`, sinon `[updates] source`) ou sur celui d’un pack (le dépôt d’où il a été importé), avec glab ou gh : l’accès que vous avez déjà, sans jeton. Ouvrez-le depuis la console (`feedback`), **Aide › Envoyer un retour…**, la barre d’état, le **?** d’un bloc (*Suggérer quelque chose pour ce bloc…*) ou **Plus** sur le téléphone.

Le formulaire montre les issues qui ressemblent à la vôtre (un 👍 plutôt qu’un doublon) et exactement ce qui part avec, chaque ligne désactivable : la version de Prism, d’où il a été ouvert, les packs et les versions des outils qu’il lance (**Tools**, tels que les liste Réglages › Général › Dépendances, un outil requis manquant nommé comme tel), cochés ; le thème et les dernières erreurs du log (chemins, adresses et e-mails retirés), non cochés. Une **capture de la fenêtre** (macOS et Linux : spectacle, grim, scrot ou import — le formulaire dit quand il n’y en a aucun), montrée dans le formulaire, est envoyée avec l’issue sur GitLab et GitHub ; sur les deux, elle est aussi dans le presse-papiers, à coller dans un commentaire. Rien de la config, des tickets ou du mail n’est jamais envoyé. **Aperçu** montre l’issue comme la forge l’affichera. **Rédiger avec un agent** demande à un agent de lire le code, de chercher un doublon et d’écrire le brouillon, qui revient dans le formulaire : l’agent n’ouvre jamais l’issue. Quand glab ou gh ne peuvent pas l’ouvrir, la page de nouvelle issue de la forge s’ouvre pré-remplie. Un brouillon est gardé jusqu’à son envoi.

**Réglages › Général › Retours** liste où vont les retours et les issues ouvertes depuis Prism. Avec **Suivre mes demandes**, leur état est relu avec les mises à jour : une issue fermée apparaît dans la barre d’état, et une release dont les notes la nomment (`#214`) affiche *Apporte votre demande* dans Mises à jour. **Labels** donne à chaque type (Fonctionnalité, Bug, Idée) son label.

[Ouvrir Réglages › Général › Retours](prism:settings/feedback)

## Ménage
**Réglages › Ménage**, en dernier dans l’arbre, dit ce que vous utilisez vraiment. Prism compte sur cette machine combien chaque fonctionnalité sert — un bloc quand une page qui le montre s’affiche, une action d’agent quand elle se lance, une entrée de la palette quand elle est choisie, la vue Agents, une réponse rapide, un fil, un balayage de page — dans le `usage.json` du profil : rien ne sort de la machine. Une fois que les compteurs couvrent 30 jours (la page dit combien d’ici là), **Activé, jamais utilisé** liste les fonctionnalités activées dont rien ne s’est servi ces 30 derniers jours : **Désactiver** écrit leur interrupteur désactivé comme n’importe quel champ (enregistré avec `⌘S`), **Garder** (cliquer deux fois) retire la ligne pour de bon. **Le plus utilisé** liste ce dont vous vous servez le plus, et **Différent du défaut** les clés que votre `config.toml` règle sur une autre valeur que leur défaut, chacune avec **Remettre** (la valeur du pack, sinon le défaut, s’applique), le reste dans le panneau **TOML**. **Masquer dans Réglages les fonctionnalités désactivées** (`[cleanup] hide_off`) retire de l’arbre les fonctionnalités désactivées ; une recherche les trouve toujours.

[Ouvrir Réglages › Ménage](prism:settings/cleanup)

## Infobulles
Prism affiche ses propres infobulles, aux couleurs du thème, avec les raccourcis sous forme de touches (`Only mine` `m`). Sous **Réglages › Interface › Apparence**, **Infobulles** règle le délai avant qu’une infobulle s’affiche (passer à la suivante l’affiche tout de suite), ou revient à celles du système.

## Où se trouvent les seuils d’alerte
Chaque seuil derrière une ligne des **Alertes** ou un badge se trouve dans **Réglages › Interface › Alertes et son** (`[alerts]`), un par règle : une branche en retard sur sa base, un job qui attend un runner, une revue qui vous attend, un ticket inactif, une session Claude en attente, ArgoCD désynchronisé, le rappel de réunion, un worktree utilisé, les seuils des pipelines de données et la taille d’une grosse MR. Ils s’appliquent tous dès l’enregistrement.

[Ouvrir Réglages › Interface › Alertes et son](prism:settings/alerts)

## L’usage de Claude et la barre d’état
La barre d’état en bas de la fenêtre, ses zones et comment l’arranger sont dans [Pages, blocs et disposition](prism:help/pages).

Le bloc **Usage de Claude** montre la fenêtre de 5 heures et la semaine de votre forfait, avec l’heure de réinitialisation. Il utilise le jeton que Claude Code garde dans le trousseau du système. Quand ce jeton a expiré, **Réglages › Agents › Agent › Claude Code › Rafraîchir le jeton** (`[agents.claude] refresh_token`, activé par défaut) laisse Prism lancer `claude doctor`, qui le renouvelle sans appeler le modèle, au plus toutes les 15 minutes.
