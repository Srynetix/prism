---
title: Clavier et console
group: Make it yours
related: settings, pages, agents, ref-keys
---
Prism se pilote entièrement au clavier, à la manière de vim, et sa console lance des commandes par leur nom. Appuyez sur `?` à tout moment pour voir tous les raccourcis sur un seul écran.

Sur Linux et Windows, Ctrl remplace la touche Commande du Mac, et Ctrl+Shift dans le terminal d’un agent, où Ctrl va à Claude.

## Se déplacer
![L’écran ? : tous les raccourcis](images/shortcuts.webp)


Un curseur marque le bloc actif et la ligne courante dans ce bloc.
- `j` / `k` (ou ↓ / ↑) passent à la ligne suivante / précédente ; au bout d’un bloc, ils continuent dans le bloc suivant de la colonne.
- `h` / `l` (ou ← / →) passent au bloc de la colonne de gauche / de droite.
- `J` / `K` (ou `⇧↓` / `⇧↑`) passent au bloc suivant / précédent.
- `gg` / `G` sautent à la première / dernière ligne ; `Ctrl-d` / `Ctrl-u` descendent / remontent de 10 lignes.

Quand vous tapez une commande à deux touches comme `gg` ou `za`, la première touche s’affiche en bas à droite pendant que Prism attend la seconde.

> [!NOTE]
> Les raccourcis sont inactifs pendant que vous tapez dans un champ. Ils ne cliquent jamais que ce que la souris cliquerait : rien ne se passe dans votre dos.

## Agir sur une ligne
- `Enter` ou `o` ouvre la ligne : un aperçu, un panneau latéral ou son lien principal.
- `O` l’ouvre dans le navigateur (ClickUp, GitLab ou GitHub).
- `a` lance l’action de la ligne : **M’assigner**, **Corriger**, **Relire**…
- `.` ouvre son menu ⋯ ; déplacez-vous avec `j` / `k`, `Enter` pour choisir, `Esc` pour sortir.
- `x` marque un élément des **Alertes** comme vu, ou à nouveau comme nouveau.

## Panneau latéral, blocs et pages
Quand une MR, un ticket, des logs ou un événement est ouvert à droite :
1. Appuyez sur `Ctrl-w l` ou `Tab` pour mettre le focus dans le panneau.
2. Faites défiler avec `j` / `k`, `Ctrl-d` / `Ctrl-u`, `gg` / `G`.
3. Parcourez ses boutons et liens avec `J` / `K`, et appuyez sur `Enter` pour en utiliser un. Dans un diff, `]` / `[` passent au fichier suivant / précédent, `n` / `N` au message suivant / précédent (un fil, ou une note à l’agent ; le bouton **Message suivant** de la barre du diff fait de même), et `v` marque le fichier courant comme vu.
4. `Ctrl-w h`, `Tab` ou `Esc` reviennent aux blocs ; un second `Esc` ferme le panneau.

Sur le bloc actif : `za` le replie ou le déplie, `zs` passe du format court au format normal, `zz` l’agrandit, et `r` relit ClickUp tout de suite (le ↻ du bloc).

Pages : `1` … `9` ou `⌘1` … `⌘9` ouvrent une page du rail (`⌘` seul sur la touche du chiffre, Shift inutile en AZERTY, pour que `⌘⇧3`/`4`/`5` restent les captures d’écran de macOS), `gt` / `gT` passent à la suivante / précédente. La vue **Agents** prend le chiffre qui suit la dernière page (`⌘6` avec cinq pages), et `⌘N` lance un nouvel agent.

## Les touches valables partout
| Touche | Effet |
|---|---|
| `/` | aller au filtre ; `Esc` le vide |
| `m` | seulement les miens : mes MR et mes tickets |
| `i` | le panneau **Pour moi** : ce qui vous attend |
| `Esc` | fermer, vider le filtre, quitter le curseur |
| `⌘,` | Réglages |
| `⌘⇧F` | Mode zen : le rail et l’en-tête de page se replient, les blocs prennent la fenêtre (aussi Présentation › Mode zen) ; l’en-tête revient quand vous filtrez (`/`) ou ouvrez Pour moi (`i`) |
| `?` | les raccourcis clavier |
| `⌘?` ou `F1` | cette Aide |
| `Ctrl+Q` / `F11` (Linux, Windows) | quitter Prism / plein écran, ce que fait là-bas la barre de menus du Mac : fermer la fenêtre ne fait que la réduire |

[Afficher les raccourcis clavier](prism:keys)

## La palette de commandes
`⌘K` ouvre un seul champ pour aller à tout ce que Prism affiche et faire tout ce qu’il sait faire. Tapez-en une partie : une page ou un bloc (`ci`), un agent (son titre, son ticket, sa branche ou ses étiquettes), une MR (`!1957`, son titre, sa branche ou son auteur), un ticket (`4688`), un pipeline, un environnement, un pod, une page ou une clé des Réglages (`tooltip_delay`), un guide ou l’un de ses titres. `↑` / `↓` choisissent, `↵` y va. Sans rien taper : ce que vous avez ouvert en dernier, ce qui vous attend (un agent qui demande, une MR à relire), les pages.

Les commandes de la console y sont aussi, avec leurs arguments : `note buy milk`, `remind demain 9h call Alice`, `agent fix the login test`, `theme nord` ; `⇥` complète le nom d’une commande, et `↵` sur une commande qui demande des arguments la tape pour vous. Ce qu’affiche une commande apparaît dans une notification, ou dans la console quand c’est une liste (`pack list`). Les commandes propres à Prism les accompagnent : Nouvel agent, Mode zen, Modifier la mise en page, Réglages, Aide, Console, Plein écran, Quitter Prism.

N’importe quel ticket s’ouvre de là, même un que Prism n’affiche pas (fermé, d’un autre board) : dès 3 caractères, ce que vous tapez est aussi cherché dans le tracker, et ses tickets rejoignent le groupe Tickets dès que vous marquez une pause (« Recherche dans le tracker… » en attendant). Un identifiant tapé (`ABC-123`) s’ouvre tel quel. ClickUp n’a pas de recherche : Prism lit une fois les tickets des espaces suivis, fermés compris, et les garde 10 min ; Jira cherche dans le projet du board. L’interrupteur : **Chercher dans le tracker** (`[palette] tracker_search`).

`⌘↵` ouvre les actions de la ligne choisie : le menu d’une MR ou d’un ticket (approuver, rebaser, merger, relancer, statuts, actions d’agent…), les réponses rapides d’un agent, Autoriser, Interrompre. Une action irréversible demande un second `↵`. `Esc` revient en arrière, puis ferme.

Dans le terminal d’un agent, `⌘K` reste celui du terminal : il l’efface. **Touches gardées par le terminal** (`[agents] terminal_keys`, Réglages › Agents › Agent) liste les raccourcis qu’un terminal garde : `Cmd+K` par défaut ; ajoutez `Cmd+N`, `Cmd+1`… pour les garder aussi, retirez `Cmd+K` pour y ouvrir la palette. La palette s’ouvre aussi depuis **Présentation › Palette de commandes…**, et sur le téléphone depuis le bouton de recherche. Son raccourci et son interrupteur : Réglages › Interface › Palette (`[palette]`).

[Ouvrir Réglages › Interface › Palette](prism:settings/palette)

## Panneaux fermés, shells restés ouverts
Un panneau latéral (une MR, un ticket, les logs d’un pipeline ou d’un pod, un shell, un événement, ArgoCD, un fil) se ferme avec `Esc`, × ou un clic à côté. `⌘⇧T` rouvre le dernier fermé, tel qu’il était : le même pod, le même conteneur et le même log (actuel ou précédent), le même job ; ce qu’il affiche est relu. Sans rien taper, la palette liste les panneaux fermés récemment sous **Fermés récemment** (les dix derniers, le temps de la session), et la commande **Rouvrir le dernier panneau fermé**.

Les shells d’un worktree ou d’un pod survivent à leur tiroir : tmux les garde. Tant qu’il en reste un, la barre d’état affiche **N shells** : un clic les liste (où, depuis quand, ce qui y tourne, « kubectl a quitté » quand le pod a disparu), une ligne rouvre son tiroir, × ferme ses shells (cliquer deux fois). La palette les liste aussi, sous **Shells ouverts**.

Pour garder un panneau ouvert pendant que vous cliquez ailleurs, activez **Épingler les panneaux latéraux** (`[appearance] pin_drawers`, Réglages › Interface › Apparence) : une épingle dans l’en-tête de chaque panneau ; épinglé, seul × (ou `Esc`) le ferme. L’épingle est retenue par type de panneau.

## La console
![La console après help](images/console.webp)


La console descend du haut de la fenêtre. Sa touche est `` ` `` par défaut (`[console] key`) ; appuyez à nouveau dessus ou sur `Esc` pour la fermer.

Tapez `help` pour lister les commandes. La console sert aux actions et aux usages avancés ; se déplacer est le rôle du clavier (pages, zoom, filtre, seulement les miens, Réglages, Aide : voir plus haut). Les commandes :
- `open !1957` ou `open ABC-4688` : ouvrir une merge request ou un ticket.
- `mr <iid> <action>` : `open`, `link`, `merge --yes`, `rebase`, `approve`, `unapprove`, `retry`, `pipeline` (un nouveau, GitLab), `logs`, `draft`, `ready`, avec les mêmes vérifications que le menu de la MR. `helm!214` désigne une MR d’un autre dépôt.
- `note <texte>` ajoute une note épinglée (`TODO …` en fait une tâche) ; `remind <quand> <texte>` un rappel (`remind demain 9h call Alice`, `remind +20m stand-up`).
- `refresh` relit ClickUp ; `rescan` lance le scan Trivy tout de suite.
- `agent <prompt>` lance tout de suite un agent sur ce prompt (`agent refine ABC-4661`) ; `agent` seul ouvre le lanceur d’agent.
- `feedback [feature|bug|idea] [titre…]` ouvre le formulaire de retour, `feedback list` les demandes ouvertes depuis Prism.
- `theme <nom>` (`theme list` pour les noms), `config` (config.toml dans l’éditeur), `pack list` / `pack update <nom>`, `restart`, `boot`, `screensaver [mode]`, `clear`.

`Tab` complète une commande et ses arguments ; `Ctrl+L` efface l’écran, comme dans un terminal. `↑` / `↓` parcourent l’historique, gardé d’un lancement à l’autre (`[console] history`, 50 lignes par défaut).

[Ouvrir la console](prism:console)

> [!TIP]
> Sur un clavier AZERTY, `` ` `` est une touche morte. Choisissez `Backquote` (la touche à gauche de 1) ou une combinaison comme `Cmd+K` dans **Réglages › Agents › Console**. Avec un modificateur Cmd ou Ctrl, le raccourci fonctionne même pendant que vous tapez dans un champ.

## Alias et notes rapides
Les **alias** sont vos propres commandes courtes, développées avant exécution. Dans **Réglages › Agents › Console › Alias**, associez `r` à `agent review`, et `r !1957` lance `agent review !1957`. Ils apparaissent dans `help` et dans la complétion.

Les **notes rapides** s’ouvrent par-dessus n’importe quelle vue avec `⌘⇧N` (`[notes] key`). `Enter` enregistre la note et garde le champ ouvert pour la suivante ; `Esc` le ferme. Les deux raccourcis acceptent un caractère, un nom de touche comme `Backquote`, ou une combinaison ; enregistrez-en un nouveau dans les Réglages.

[Ouvrir Réglages › Agents › Console](prism:settings/console)
