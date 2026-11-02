---
title: Thèmes, polices et petits plaisirs
group: Make it yours
related: settings, keyboard, welcome
blocks: shipit, quotes, pet
---
Donnez à Prism l’allure qui vous plaît, et décidez à quel point il est joueur : thèmes, polices, écran de démarrage, célébrations, économiseur d’écran et un peu de musique. Chaque touche a son propre interrupteur dans **Réglages › Interface › Apparence** (`[appearance]`).

## Thèmes
![Le menu Thème du rail](images/themes.webp){width=220}


Cliquez sur **Thème** en bas du rail et choisissez-en un : le menu se ferme dessus. Le choix est enregistré tout de suite dans `config.toml` (`[appearance] theme`). **Tous**, **Clairs** et **Sombres** en haut ne gardent que les thèmes de ce type (le menu s’en souvient ; le thème choisi reste toujours listé). Avant de choisir, survolez un thème (ou atteignez-le avec `Tab`) pour l’essayer sur toute la fenêtre ; quitter le menu, ou le fermer avec `Esc`, remet celui que vous aviez. Réglages › Interface › Apparence › **Aperçu du thème** (`[appearance] theme_preview`) désactive ce comportement.

- **Système** suit le système : Paper le jour, Graphite la nuit.
- La famille par défaut : **Paper** (clair), **Graphite** (sombre), **Fjord** (sombre et froid), **Midnight** (bleu profond), **Sand** (clair et chaud) et **Contrast** (contraste élevé).
- Des palettes populaires, groupées par famille : Catppuccin, One Dark, Dracula, Tokyo Night, Nord, GitHub, Gruvbox, Solarized et Rosé Pine ; et pour le plaisir, **Hacker** (phosphore vert), **Synthwave** (rose et cyan néon), **Plan** (un plan d’architecte) et **Quête** (un RPG 8-bit). Cliquez sur le nom d’une famille pour la replier.

Un thème change plus que les couleurs : chacun a sa forme et sa profondeur. Catppuccin est arrondi et doux, sans contours durs. Hacker est un vieux terminal : monospace, anguleux, lignes de balayage. Synthwave a des blocs de verre au-dessus d’un horizon néon. Plan est tracé en traits blancs fins sur un quadrillage, carré, titres en capitales espacées. Quête fait de chaque bloc une fenêtre de dialogue au cadre blanc, son titre en police pixel. Sand se lit comme un journal, avec des titres à empattements au-dessus d’un filet. Contrast a des contours épais. One Dark et GitHub mettent une barre d’en-tête sur chaque bloc. Une police choisie dans **Police** l’emporte toujours sur celle du thème.

Vous pouvez aussi taper `theme <nom>` dans la console ; `theme list` affiche les noms.

La **Densité** (`[appearance] density`, Réglages › Interface › Apparence) est indépendante du thème : **Compacte** telle que dessinée, ou **Confortable**, avec un texte plus grand et des lignes plus aérées.

## Polices
Des champs de **Réglages › Interface › Apparence** règlent les polices, chacun avec un aperçu en direct :
- **Police** (`[appearance] font`) : le texte de l’application. Vide : Inter, fournie avec l’application (la même sur macOS et Linux).
- **Police monospace** (`[appearance] font_mono`) : identifiants, branches, code et terminal des agents. Vide : Recursive Mono, intégrée à Prism.
- **Police du terminal** (`[appearance] font_terminal`) : les terminaux des agents et leurs shells uniquement. Vide : RecMonoCasual Nerd Font Mono, intégrée à Prism, une Nerd Font dont les icônes dessinent votre prompt.

Une police que vous avez installée vous-même (dans `~/Library/Fonts`, ou via un gestionnaire de polices) fonctionne dans les trois : macOS la cache à la vue web de Prism, alors Prism retrouve ses fichiers par son nom de famille et les charge lui-même.

Les suggestions listent d’abord les polices installées sur votre ordinateur. Une police non installée affiche un avertissement, et la police par défaut est utilisée à la place.

[Ouvrir Réglages › Interface › Apparence](prism:settings/appearance)

## Démarrage, animations et célébrations
- **Écran de démarrage** (`boot`) : un écran d’accueil à chaque lancement, pendant que les sources répondent : le prisme respire, l’arc-en-ciel s’allume bande par bande à mesure que les sources répondent, chaque ligne se tape d’elle-même, des étoiles dérivent derrière. Il prend les couleurs du thème, toujours sombre ; Synthwave est une course façon Outrun vers un soleil qui se lève à mesure que les sources répondent, chacune un point de passage, **GOAL!** à la fin ; Hacker un terminal vert qui démarre, avec **ACCESS GRANTED** à la fin ; Plan un plan où le prisme se trace trait par trait, chaque source une cote numérotée, tamponné **APPROUVÉ** à la fin ; Quête un RPG où chaque source rejoint l’équipe dans la boîte de dialogue, la barre d’XP se remplit, **LEVEL UP!** à la fin. **Rejouer maintenant** dans les Réglages, ou `boot` dans la console, le montre à nouveau.
- **Animations** (`animations`) : l’icône de rafraîchissement, Claude qui attend ou travaille, le stage en cours, les compteurs qui défilent, un bloc déplacé qui s’incline.
- **Mouvement du thème** (`theme_motion`) : les deux thèmes néon s’animent. Synthwave : un soleil rayé qui respire sur une grille qui avance vers vous, des étoiles qui scintillent, le bord du rail qui coule du rose au cyan, un reflet chromé sur un agent qui travaille et sur les boutons principaux, une ligne qui arrive en balayage rose → cyan. Hacker : un curseur bloc qui clignote après chaque titre, un balayage cathodique qui descend la fenêtre, un phosphore qui scintille à peine avec des bords plus sombres, une pluie de katakanas pâle derrière les blocs, un spinner ASCII sur un agent qui travaille, une ligne qui arrive tapée, une nouvelle alerte qui tremble.
- **Célébrations** (`celebrations`) : des confettis quand votre MR est mergée, une petite gerbe verte quand le pipeline de votre MR repasse au vert après un échec, des confettis depuis la pastille de temps une fois toutes les heures du jour saisies (`[clickup] day_hours`, une fois par jour), un feu d’artifice une fois par sprint quand tous les tickets hauts et urgents sont terminés. Quand les **Alertes** sont vides, une pousse y grandit, avec une feuille de plus pour chaque jour comme celui-là, jusqu’à sept.

> [!NOTE]
> Quand le système demande de réduire les animations ou la transparence (**Réduire les animations**, **Réduire la transparence** sur macOS, Accessibilité › Affichage), animations et célébrations restent immobiles quel que soit leur réglage, et le verre et les superpositions d’un thème sont désactivés. L’écran de démarrage s’affiche quand même, mais immobile : pas d’étoiles, lignes entières, l’arc-en-ciel allumé d’un coup. **Ignorer Réduire les animations et la transparence** (`ignore_reduce`) les montre malgré tout.

## Easter eggs et garde du vendredi
Avec **Easter eggs** activé (`easter_eggs`) :
- Cliquez sept fois rapidement sur le logo : il tourne et projette un arc-en-ciel le long du rail.
- Maintenez `⌥` seul un instant : chaque bloc montre son identifiant et d’où viennent ses données.

La **garde du vendredi** (`friday_guard`) surveille les boutons qui lancent un job de production, comme un tag de production. Le vendredi à partir de 16 h, les deux clics habituels ne suffisent plus : le bouton passe au rouge et en demande un de plus.

> [!TIP]
> Tous ces interrupteurs s’appliquent dès que vous enregistrez les Réglages avec `⌘S`.

## Blocs inutiles
Trois blocs qui ne servent absolument à rien, à mettre sur une page avec **Modifier la mise en page › + Bloc** :

- **On livre ?** : faut-il déployer maintenant ? Non le vendredi après-midi, le soir, le week-end, quelques minutes avant une réunion, avec un pipeline rouge ou une alerte rouge. Sinon, oui.
- **Citations** : une citation sur la programmation et son auteur (Knuth, Dijkstra, Hopper, Brooks…), une nouvelle chaque heure ; **Une autre** pour en changer, **Copier** pour la coller.
- **Compagnon** : un compagnon virtuel. Trois besoins baissent avec le temps, même Prism fermé : la faim, l’énergie et l’amusement. **Nourrir**, **Jouer** (il saute après la balle), cliquez dessus pour le caresser, **Dormir** éteint sa lumière pour une sieste : son énergie se recharge en une demi-heure environ (la nuit, il dort plus longtemps et récupère plus lentement). Chaque soin et chaque MR mergée donne de l’expérience : il monte de niveau et change d’allure en grandissant (bébé, jeune, adulte, légende). Il réagit à votre travail : malade avec un pipeline protégé rouge, inquiet face aux alertes rouges, il vous prévient quand un agent vous attend ou qu’une réunion va commencer, et fête les merges. Il dort la nuit et le week-end ; ses yeux suivent votre pointeur ; cliquez sur son nom pour le renommer.

### Le compagnon grandit avec votre travail
![Devenu Le Relecteur, dans sa chambre : une plante, le soleil, une coupe gagnée ; le reste encore en contours](images/pet-room.webp){width=336}

Avec la Trace du jour activée (Réglages › Travail › Trace du jour), le compagnon lit aussi vos traces. Trois interrupteurs dans Réglages › Interface › Compagnon (`[pet]`), tous activés par défaut :

- **Formes** (`forms`) : adulte (niveau 6), il prend la forme de ce que vous avez le plus fait ces 14 derniers jours, et le dit. **Le Relecteur** porte des lunettes et un diff (revues et commentaires), **Le Livreur** se pare de nageoires et d’une flamme (MR mergées), **Le Réparateur** tient une clé (correctifs et hotfixes), **Le Noctambule** porte un bonnet de nuit (une bonne part du travail après 20:00), **L’Architecte** un casque de chantier (affinage, ADR, architecture). Aucune forme n’est mauvaise. Elle change au plus une fois par semaine, si votre travail change.
- **Trophées** (`trophies`) : des étapes placent un objet dans sa chambre : une plante pour dix merges, le soleil à la fenêtre pour cinq jours ouvrés sans pipeline protégé rouge, une coupe pour un sprint fini avec tous vos tickets terminés, des livres pour cinquante revues, une fusée pour dix MR de moins de 50 lignes, une balle après un mois ensemble, une chouette pour un commit après 22:00, un oiseau pour un commit avant 7:00, une étoile au niveau 10. Ceux qui restent à gagner sont des contours pâles. **🏆** dans son titre les liste, avec votre progression.
- **Journal** (`diary`) : le vendredi après-midi (ou la prochaine fois que vous ouvrez Prism, si vous l’avez manqué), il écrit une page sur votre semaine, jour par jour, avec ses propres mots. **📔** dans son titre l’ouvre ; ‹ › tournent les pages (les huit dernières sont gardées).

![Ses trophées, gagnés et à gagner](images/pet-trophies.webp)

![Une page de son journal](images/pet-diary.webp)

### Ses amis, sa fenêtre, son album
Cinq interrupteurs de plus au même endroit, eux aussi activés par défaut :

- **Amis** (`friends`) : les agents en marche s’assoient dans sa chambre, jusqu’à trois, chacun à la couleur de sa CLI (au-delà : **+n**, la vue Agents). Celui qui vous attend sautille avec un **!**, et le compagnon le nomme ; un clic ouvre l’agent.
- **Rapporte** (`fetch`) : un quatrième soin, **🦴 Rapporte**. Il part en courant et revient avec la prochaine chose qui vous attend, sur un papier dans la gueule : d’abord un agent qui vous attend, puis la plus ancienne revue qu’on vous demande, puis vos MR bloquées ou à traiter (CI rouge, conflits, modifications demandées), puis celles prêtes à merger. Un clic sur le papier l’ouvre ; Rapporte à nouveau ramène la suivante. Traitée dans l’heure : +5 XP.
- **Saisons** (`seasons`) : sa fenêtre suit l’heure (l’aube, le jour, le soir, une nuit étoilée) et la saison (la neige l’hiver, les feuilles l’automne, les fleurs au printemps). Quelques jours sont spéciaux : son anniversaire (un an après son éclosion), le dernier jour du sprint, le vendredi après-midi (un panneau **Pas de déploiement**), Halloween, Noël, le Nouvel An.
- **Dans la barre d’état** (`status_bar`) : quand son bloc est sur une page mais pas celle affichée, un petit compagnon attend dans la barre d’état, et sautille quand il a quelque chose à dire. Un clic ouvre sa ligne courte : ce qu’il dit, ses besoins, ses soins. Un clic droit sur la barre d’état le masque comme tout élément.
- **Album** (`album`) : une photo à chaque étape (son éclosion, chaque stade, chaque nouvelle forme, chaque trophée), tel qu’il était alors. **📷** dans son titre ouvre l’album ; la dernière photo est accrochée à son mur (un clic ouvre aussi l’album).

[Ouvrir Réglages › Interface › Compagnon](prism:settings/pet)

## L’économiseur d’écran
Après un moment sans touche, clic ni mouvement de souris, quand la fenêtre est visible, Prism lance un économiseur d’écran. Réglez le délai avec **Économiseur d’écran** (`screensaver_seconds`, 600 s (10 minutes) par défaut), ou désactivez-le (`screensaver`) : il ne démarre alors qu’à la demande.

Cinq modes, plus l’aléatoire, choisis avec **Économiseur affiché** (`screensaver_mode`) :
- **spectrum** : le prisme et son arc-en-ciel (par défaut).
- **bounce** : le logo rebondit et change de couleur à chaque bord ; toucher un coin pile compte, et votre record est gardé.
- **matrix** : une pluie de code écrite avec les tickets, MR et branches du moment.
- **stars** : un champ d’étoiles en hyperespace.
- **life** : le jeu de la vie.
- **random** en choisit un à chaque fois.

Pour en essayer un :
1. Ouvrez la console et tapez `screensaver matrix` (ou un autre mode ; seul, il utilise votre réglage). **Essayer maintenant** dans les Réglages fait de même.
2. Appuyez sur `←` / `→` pour passer au mode précédent / suivant.
3. Cliquez ou appuyez sur une autre touche pour le réveiller. Rien n’est perdu.

## Audio
Le bloc **Audio** de la barre d’état joue quelque chose pendant que vous travaillez : stations de radio, vidéos YouTube, lives ou playlists, et podcasts. Sans configuration, il propose une sélection intégrée ; les listes que vous donnez dans **Réglages › Interface › Alertes et son** (`[audio]`) la remplacent. Sur macOS, il reprend aussi ce que le système affiche comme en cours de lecture (la musique de votre navigateur, Musique…), et vous laisse le mettre en pause ou passer au suivant depuis la barre. Le **×** de son menu arrête et désélectionne ce que joue Prism : la barre revient à *Audio* (un épisode garde sa position).

YouTube refuse un lecteur intégré sans referrer, ce que la page de l’application ne peut pas envoyer : Prism l’intègre dans une page d’un petit relais à lui, sur `http://localhost` et un port choisi par le système, démarré quand vous lisez une vidéo pour la première fois. Il ne sert que cette page, et à la machine seulement.

[Ouvrir Réglages › Interface › Alertes et son](prism:settings/audio)
