[English](README.md) | [日本語](README.ja.md) | [简体中文](README.zh-cn.md) | [繁體中文](README.zh-tw.md) | [한국어](README.ko.md) | [Español](README.es.md) | [Português (Brasil)](README.pt-br.md) | **Français** | [Deutsch](README.de.md) | [Русский](README.ru.md)

# sim, le Vim nouvelle génération

**sim** (aussi appelé *sim editor*, abréviation de *Secure vIM*) est le Vim nouvelle génération : écrit à partir de zéro en Rust, il conserve l’édition modale et les touches de base de Vim. Il ne lit aucune modeline, ne charge aucun plugin et n’ouvre aucune connexion réseau, et il met à l’écran ce qui reste d’ordinaire caché : caractères invisibles, écritures venues de l’extérieur de l’éditeur, dépôts qui exécuteraient du code.

**Pas de plugins. Pas de modelines. Pas de réseau.**

Site web : <https://sim.efg-technologies.com/fr/> · Linux et macOS · MIT OR Apache-2.0 · [Journal des modifications](CHANGELOG.md) · [Politique de sécurité](SECURITY.md)

Tout ce que sim promet, il le promet pour chaque fichier ouvert dans l’éditeur : **sans action explicite, le texte de personne n’est perdu**, ni le vôtre ni celui d’un autre processus.

## Installation

Le binaire est `sim`. Le nom du paquet est `sim-editor` sur l’AUR et crates.io, et `efg-technologies/tap/sim` sur Homebrew ; la bibliothèque sur laquelle il repose est `sim-editor-core`, sur crates.io.

sim 0.1.0 est publié. Installez-le avec Homebrew :

```sh
brew install efg-technologies/tap/sim
```

Le code source de la version se trouve sur la [page de la version v0.1.0 sur GitHub](https://github.com/efg-technologies/sim/releases/tag/v0.1.0) (`sim-0.1.0.tar.gz`). Pour le compiler depuis les sources :

```sh
git clone https://github.com/efg-technologies/sim.git
cd sim
cargo install --locked --path sim
```

Le paquet `sim-editor` de l’AUR et les crates `sim-editor` et `sim-editor-core` de crates.io arrivent bientôt.

Ensuite, si vous l’adoptez comme éditeur :

```sh
export EDITOR=sim                     # Ctrl-g de Claude Code, crontab -e, …
git config --global core.editor sim   # git commit (ZZ valide, :cq annule)
export SUDO_EDITOR='sim --plain'      # sudoedit (sous Linux, sim y est déjà en mode plain de lui-même)
```

Nécessite Rust 1.88 ou plus récent. Fonctionne sous Linux et macOS, dans n’importe quel terminal ; les couleurs, le protocole clavier de kitty et OSC 52 sont utilisés quand le terminal les propose (Ghostty, kitty, WezTerm, tmux). git et ripgrep sont facultatifs : sim les utilise quand ils sont dans le `PATH`. Les icônes nécessitent une Nerd Font ; elles démarrent activées dans Ghostty, kitty et WezTerm et désactivées ailleurs, avec une astuce affichée une seule fois, et `zi` les active et les désactive. Plus de détails sur la [page d’installation](https://sim.efg-technologies.com/fr/install/).

## Ce que sim fait

**Montre ce qui est caché**

- Un forçage de droite à gauche, une espace de largeur nulle ou un caractère de contrôle est dessiné sous la forme `<U+202E>` ou `^[` sur un bloc magenta, dans tous les états où une cellule peut se trouver, et n’est jamais envoyé tel quel au terminal. Le texte littéral `<U+202E>` dans un fichier ne reçoit aucun bloc.
- Tout le reste de ce qui ne se voit pas est dessiné de la même façon : un caractère d’étiquette (c’est ainsi qu’on glisse une instruction à un modèle de langage), un sélecteur de variation (c’est ainsi que du code a été caché dans des paquets publiés), un remplisseur hangul ou un trait d’union conditionnel dans un nom, un liant de largeur nulle qui ne lie rien, un séparateur de ligne, un point de code que personne n’a attribué. Les noms de l’arbre, des onglets, de la recherche et de la ligne d’état sont dessinés comme le texte. Les séquences d’émoji, les drapeaux, les touches et les icônes d’une Nerd Font restent telles quelles. Le sélecteur qui suit un kanji est montré aussi.
- Les espaces en fin de ligne s’affichent en `·`, une espace insécable en `⍽`, une tabulation en `»`.
- Quand un autre processus réécrit le fichier ouvert, les lignes modifiées clignotent un instant puis sont signalées dans le texte, l’encadré indique combien de modifications sont sur le disque, l’onglet affiche `↯` et l’arborescence marque le fichier. `]c` et `[c` mènent à la marque suivante et à la précédente.
- Les écarts avec git HEAD apparaissent sous forme de signes dans la marge, d’une lettre d’état pour chaque fichier de l’arborescence (un dossier affiche la lettre de l’état le plus fort en dessous de lui) et de `+n ~n -n` dans la ligne d’état.
- git n’est pas exécuté là où la configuration du dépôt lui-même pourrait lancer une commande (`core.fsmonitor`, un pilote de filtre, et d’autres) : l’encadré affiche `git not run` sur le bloc d’alerte, et la ligne d’état indique la clé et le fichier. Là où git s’exécute avec des réglages du dépôt désactivés (`core.fsmonitor`, le chemin des hooks, des filtres ou un diff externe), l’encadré en donne le nombre (`2 git settings off`).
- Un lien Markdown dont le texte nomme une adresse que sa cible n’est pas (`[https://good.example](https://evil.example)`, aussi un lien de référence `[good.example][1]` dont la définition mène ailleurs, un lien sur plusieurs lignes ou avec une barre oblique inverse ou une entité dans la cible, un lien HTML `<a href="…">`) a sa cible, et pour une référence son texte, sur le bloc magenta, dans la vue de lecture et aussi en calm ; la vue de lecture ne cache jamais les crochets ni l’URL d’un lien.

**Refuse ce qu’il doit refuser, conserve votre texte**

- Les réglages et les commandes ne sont jamais lus dans le contenu des fichiers : pas de modelines, pas de fichiers de configuration, pas de plugins, pas de réseau.
- Les enregistrements sont atomiques (fichier temporaire, fsync, renommage, fsync du répertoire) et vérifient d’abord l’empreinte (hash) du fichier : si le fichier a changé sur le disque, rien n’est écrasé. Quand un enregistrement constate un conflit, `:w!` conserve la vôtre après avoir copié celle du disque dans le répertoire d’état, et `:e!` adopte celle du disque après un instantané de la vôtre.
- Les modifications non enregistrées sont sauvegardées en instantané chaque seconde ; après un plantage, un `kill -9` ou la fermeture du terminal, rouvrir le fichier les propose de nouveau en une seule étape d’annulation.
- La recherche trouve ripgrep elle-même, l’exécute avec un environnement vide et `--no-config`, et ne suit jamais un lien symbolique hors du projet.
- `sim --plain` n’est que le cœur : un seul fichier, aucun panneau et aucun programme externe (ni git, ni ripgrep, ni filtre, ni hook). C’est le mode par défaut en root et comme éditeur de sudoedit.

**Accès, vitesse et lecture**

- Une arborescence de projet qui ne bouge jamais, `Ctrl-p` pour trouver des fichiers, `Ctrl-f` pour chercher du texte avec aperçu, les fichiers probables marqués d’un `•`, plusieurs fichiers avec une barre d’onglets.
- Dans le benchmark du projet lui-même, une touche prend moins de 1 ms au p99 (fichier de 100 000 lignes, build de release) ; la première image dans un dépôt de 100 000 fichiers a été mesurée entre 1,8 et 8,7 ms.
- Des couleurs de syntaxe tirées de la palette de votre terminal, un Markdown lu sur place (marques masquées, tableaux alignés, et la ligne où vous êtes toujours le texte du fichier), trois apparences (`zl`), le retour à la ligne japonais selon les règles kinsoku et les touches pleine chasse.
- Le titre de l’éditeur est un fil d’Ariane, `src › sim.rs › leader_key` : le chemin et le titre ou la fonction où se trouve le curseur. Chaque fait est à un seul endroit : la ligne d’état a le mode, la branche, les compteurs et la position, et ne dit `CRLF`, `BOM` ou un fichier qui n’est pas en UTF-8 que lorsque c’est le cas ; l’onglet a le fichier et `●`, la barre de défilement la position, et l’arborescence n’est pas dans le brouillard.
- `NO_COLOR` et les terminaux de moins de 8 couleurs dessinent uniquement avec l’épaisseur, le soulignement et la vidéo inverse, et le sens ne dépend jamais de la couleur seule : une ligne modifiée est `┆`, les dossiers affichent des lettres d’état, un onglet affiche `↯` pour un fichier modifié sur le disque, et la poignée de la barre de défilement est un rail `║`, non un bloc inversé.

**Des touches faciles à trouver**

- `Space` est la touche leader. Une pause sur un préfixe (`Space`, `g`, `z`, `Z`, `[`, `]`, ou un opérateur comme `d`) ouvre une petite fenêtre which-key qui liste les touches qui peuvent suivre ; continuer à taper fonctionne exactement comme sans elle.
- `:help` et `F1` ouvrent une liste consultable de chaque touche et commande, avec ce que fait chacune ; `:help tab` trouve `Tab`, `Shift-Tab` et `<Space>bp`.
- `:tutor` ouvre environ quinze minutes de leçons courtes dans une copie jetable (rien n’est écrit), avec des exercices qui se cochent ✓ tout seuls quand vous les réussissez ; il parle votre langue (`LC_ALL`, `LC_MESSAGES`, `LANG`), l’une des dix de ce README.
- `<Space>bp` place une lettre sur chaque onglet, et la lettre mène à son fichier ; `<Space>bc` fait de même pour en fermer un (un fichier modifié reste ouvert, comme avec `:q`). Une astuce nomme `Tab` et `Shift-Tab` la première fois qu’un deuxième fichier est ouvert, jusqu’à ce que vous les ayez utilisées trois fois.
- Tant que les occurrences d’une recherche sont mises en évidence, la ligne d’état indique sur quelle occurrence vous vous trouvez et combien il y en a, `[3/12]`.
- `sim --help` affiche l’usage et l’environnement que lit sim (`NO_COLOR`, `SIM_ON_NORMAL`, `XDG_STATE_HOME`) ; `sim --version` affiche la version. Ni l’un ni l’autre ne touche au terminal.

## sim en un seul écran

| Touches | Ce qu’elles font |
|---|---|
| `i` | insérer |
| `Ctrl-s` | enregistrer |
| `ZZ` | enregistrer et quitter |
| `q` | quitter |
| `ZQ` | quitter sans enregistrer |
| `u` | annuler |
| `U` | rétablir |
| `Y` | copier jusqu’à la fin de la ligne |
| `/` | rechercher vers l’avant |
| `n` | occurrence suivante |
| `*` | rechercher le mot vers l’avant |
| `yy` | copier des lignes |
| `"a` | un registre : `"ayw` copie dans `a`, `"ap` le colle (`:registers` les liste) |
| `ma` | poser une marque ; `'a` va à sa ligne, `` `a `` à son emplacement ; `:marks` les liste |
| `]c` | changement suivant venu de l’extérieur |
| `[c` | changement précédent venu de l’extérieur |
| `:s` | substituer |
| `:{range}!cmd` | filtrer |
| `:seen` | effacer les marques de changement |
| `Ctrl-n` | afficher ou masquer l’arborescence |
| `Ctrl-p` | trouver des fichiers |
| `Ctrl-f` | chercher dans le contenu des fichiers |
| `.` | fichiers modifiés et récents (dans l’arborescence) |
| `Tab` | fichier suivant |
| `Shift-Tab` | fichier précédent |
| `zh` | couleurs de syntaxe activées ou désactivées |
| `zl` | apparence suivante |
| `zi` | icônes activées ou désactivées |
| `<Space>` | leader |
| `<Space>bp` | choisir un onglet par une lettre |
| `<Space>bc` | choisir un onglet à fermer |
| `<Space>bk` | où est allée la version abandonnée (`:w!`, `:e!`) |
| `<Space>ge` | voir les changements par rapport à HEAD |
| `<Space>gi` | ce que la garde a fait à ce dépôt |
| `<Space>um` | vue de lecture Markdown activée ou désactivée |
| `F1` | aide |
| `:help` | aide |

Tout le reste, c’est vi : déplacements, opérateurs, objets de texte, compteurs, `.`, mode Visuel, `:s`, plages. `Y` copie jusqu’à la fin de la ligne, `U` rétablit, et chaque copie (yank) sans nom de registre atteint aussi le presse-papiers du système via OSC 52, y compris à travers tmux (`"ay`, avec un nom, reste dans sim). Rouvrir un fichier vous replace là où vous l’aviez laissé. Quand l’IME est activé, les touches pleine chasse fonctionnent comme des commandes ; `SIM_ON_NORMAL='fcitx5-remote -c'` désactive l’IME chaque fois que vous revenez en mode Normal (`SI_ON_NORMAL`, son ancien nom, fonctionne toujours). Toutes les touches sont dans une seule table, `sim/keys.tsv`, que `:help` et which-key lisent et que les tests comparent à ce que font les touches. La liste complète se trouve sur la [page de documentation](https://sim.efg-technologies.com/fr/docs/).

## Modèle de sécurité

Vous ouvrez des fichiers et des dépôts que vous n’avez pas écrits. L’attaquant contrôle leur contenu, pas votre machine. sim s’efforce de garantir que ce contenu ne puisse pas se cacher de vous, ne puisse pas reconfigurer l’éditeur et ne puisse pas lui faire exécuter du code. sim **n’est pas un bac à sable** : il s’exécute avec vos privilèges, il ne détecte pas les lettres d’apparence identique issues d’autres écritures, et ses affirmations viennent de la spécification et des tests du projet lui-même, pas d’un audit indépendant. Les serveurs de langage ne sont pas encore pris en charge ; un client dans un bac à sable est prévu. Pour signaler une vulnérabilité, lisez la [politique de sécurité](SECURITY.md).

Le modèle de menaces complet, menace par menace, se trouve sur la [page de sécurité](https://sim.efg-technologies.com/fr/security/). Trois longs articles vont plus loin : les [caractères cachés et bidirectionnels](https://sim.efg-technologies.com/blog/hidden-characters/), les [dépôts qui exécutent du code](https://sim.efg-technologies.com/blog/repositories-that-run-code/) et les [enregistrements sûrs et les secrets](https://sim.efg-technologies.com/blog/safe-saves/).

## Comparé à Vim, Neovim et Helix

Vim, Neovim et Helix sont des éditeurs matures et sim est nouveau. Seul ce qu’affirme une source primaire est comparé ; la [page de comparaison](https://sim.efg-technologies.com/fr/compare/) cite tout (vérifié le 2026-10-17 : Vim 9.2.1202, Neovim 0.12.6, Helix 25.07.1).

| | Vim | Neovim | Helix | sim |
|---|---|---|---|---|
| Extensions | Vim script, paquets | Lua, Vim script | aucun système de plugins dans une version publiée ; la PR Steel [#8675](https://github.com/helix-editor/helix/pull/8675) est un brouillon | aucune, par conception |
| Modelines | activées (désactivées pour root) ; `'modelinestrict'` depuis [9.2.0350](https://github.com/vim/vim/commit/4c2879471) | activées (désactivées pour root) | non analysées ([#729](https://github.com/helix-editor/helix/issues/729)) | jamais lues |
| Configuration locale au projet | `'exrc'`, désactivée par défaut | `'exrc'`, liste de confiance | `.helix/config.toml` chargé dans la 25.07.1 ; la confiance dans l’espace de travail est sur la branche de développement | n’existe pas |
| U+202E et compagnie | `<202e>` | `<202e>` | aucun marqueur dans la 25.07.1 (notre test ; [#11255](https://github.com/helix-editor/helix/issues/11255)) | `<U+202E>` sur un bloc magenta |

Là où les autres ont de l’avance, pour l’instant : Neovim et Helix intègrent un client de serveur de langage, Vim et Neovim ont des plugins et des macros. sim n’a encore rien de tout cela.

## Feuille de route (prévu, pas livré)

Macros de Vim · intégration du presse-papiers au-delà d’OSC 52 · un client de serveur de langage dans un bac à sable · `:tutor` · une arborescence des diffs git · une interface graphique avec une vraie profondeur (un prototype) · des paquets pour l’AUR et crates.io.

## Développer sim

sim est un seul programme en deux crates : le **cœur** (`sim-editor-core`, dans `core/` : le tampon, la grammaire de vi, les enregistrements, la fusion avec ce que d’autres processus écrivent, le terminal) et l’**éditeur** (`sim-editor`, dans `sim/` : le binaire `sim`, avec l’arborescence, la recherche de fichiers et de texte, les marques, la syntaxe, les apparences et la souris). Le pourquoi de chaque choix est dans [CONCEPT.md](CONCEPT.md), avec les recherches sur lesquelles il repose. Ce que sim doit faire est dans [SPEC.md](SPEC.md) : chaque exigence a un identifiant, et chaque identifiant a des tests.

La pyramide : [CONCEPT](CONCEPT.md) (pourquoi) → [SPEC](SPEC.md) (quoi, avec identifiants) → `core/src` et `sim/src` (comment) → tests (preuve). Les changements descendent ; la preuve remonte. [CLAUDE.md](CLAUDE.md) contient les règles.

```sh
sh scripts/check.sh          # fmt, clippy, tests, lien SPEC↔tests, les deux plafonds de lignes, dépendances du cœur, taille du binaire
sh scripts/e2e.sh            # le cœur (en `sim --plain`) et l’éditeur dans tmux (socket dédié)
cargo test --release --workspace -- --ignored bench   # exigences de performance
```

Des tests de propriétés, des fuzzers dotés d’un modèle de non-perte du texte de chaque partie, l’injection de fautes à chaque étape d’un enregistrement, un oracle différentiel avec nvim et des fichiers de référence (goldens) de l’écran rendu gardent les promesses honnêtes.

## Licence

MIT OR Apache-2.0, au choix : [MIT](LICENSE-MIT) ou [Apache-2.0](LICENSE-APACHE). Copyright EFG Technologies.
