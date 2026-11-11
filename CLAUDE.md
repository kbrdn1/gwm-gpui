# gwm-gpui

App desktop native pour [gwm](https://github.com/kbrdn1/gwm-cli), en Rust sur GPUI (`gpui-pre`, snapshots de Zed). macOS uniquement pour l'instant.

## Stack & commandes
⚠️ `export PATH="$HOME/.cargo/bin:$PATH"` d'abord : un rustc 1.89 du profil nix peut passer avant rustup, qui lui suit `rust-toolchain.toml` (1.99.1 ; `gpui-pre` 0.3.11 exige ≥ 1.98).

- Build : `cargo build`
- Test  : `cargo test` (l'app et `gwm-client`) · `cargo test -p gwm-client` (la couche gwm seule, en quelques secondes, sans compiler GPUI). Les tests de `gwm-client` pilotent un vrai `gwm daemon` : `gwm` doit être sur le `PATH`, sinon ils échouent (exprès, ils ne se sautent pas).
- Lint/Format : `cargo clippy --workspace --all-targets --all-features -- -D warnings` (comme la CI : `--all-features` vérifie aussi l'example de capture) · `cargo fmt --all`
- Audit : `cargo audit --deny warnings` (lit `.cargo/audit.toml`)
- Run   : `cargo run -- <chemin-du-repo>` (par défaut le dossier courant s'il est dans un repo, sinon un sélecteur de dossier). `gwm` doit être sur le `PATH`.
- Bundle : `scripts/bundle.sh` → `gwm.app` non signé dans `$CARGO_TARGET_DIR/release/` (build `--release` : recompile GPUI, plusieurs Go). Icône dessinée depuis `assets/app-icon.svg` par `sips` + `iconutil`, rien hors de macOS. Lancé du Finder : `open <…>/gwm.app`, ou `--args <repo>`.

Premier build long (GPUI compile tout) : `nice -n 10 cargo build -j 4` pour ne pas saturer la machine.

Voir l'UI sans écran : `cargo run --example screenshot --features screenshot -- <repo> <out.png> [--row n] [--no-sidebar] [--keys "down down"] [--click x,y] [--live ms]`. `--keys` et `--click` passent par les vrais bindings et le vrai focus, se répètent et s'appliquent dans l'ordre donné (`--click` sur un champ, puis `--keys "4 2"` y tape) ; `--live` suit le repo comme l'app (thread `watch`, daemon lancé puis arrêté). Rend la fenêtre via `Window::render_to_image` (texture Metal, sans permission d'enregistrement d'écran). La feature tire le `test-support` de GPUI, jamais dans l'app ; la CI passe clippy en `--all-features` pour que l'example ne pourrisse pas. Le PNG garde l'alpha : le poser sur un fond (`magick fond.png rendu.png -composite`, fond en **sRGB**, un `gradient:` est en niveaux de gris et désature tout) pour juger. Le flou du window server n'y est pas.

## Structure
- `src/main.rs` : bootstrap de l'application (`PATH` du shell de login si `gwm` n'est pas sur celui hérité, repo : argument, sinon dossier courant s'il est dans un repo, sinon sélecteur de dossier ; assets, touches, fenêtre, arrêt du daemon au `on_app_quit`). Tout le reste est en lib (`src/lib.rs` : `window_options`, `bind_keys`), pour que l'example de capture ouvre la même fenêtre.
- Vues, un fichier chacune : `src/workspace.rs` (vue racine : état, sélection, cadre « rounded card » et ses coins peints), `src/title_bar.rs`, `src/sidebar.rs`, `src/detail.rs` (et ses actions : ouvrir dans…, supprimer), `src/open_menu.rs` (popover « Open in… »), `src/create_form.rs` (formulaire « New worktree » à la place de la carte, ouvert par le `+` de la barre de titre), `src/text_input.rs` (champ d'une ligne, réduit de l'`examples/input.rs` de GPUI, Apache-2.0, voir `NOTICE` ; contexte de touches `TextInput`), `src/status_bar.rs`. Logique sans fenêtre testée : `State`, `order` et `removal_prompt` (workspace), `sections` (sidebar), `src/age.rs` (format et groupes par jour, ceux de Monocode), `src/apps.rs` (éditeurs et terminaux installés, par leur bundle `.app` comme Monocode, ouverts par `/usr/bin/open -a`).
- `src/login_shell.rs` : le `PATH` du shell de login (`$SHELL -l -i`, valeur entre marqueurs, timeout de 5 s, groupe tué au-delà), pour un lancement Finder qui n'a que celui de launchd. D'après `scm/shell.rs` de Monocode.
- `scripts/bundle.sh` + `assets/app-icon.svg` : le `gwm.app` et son icône (tuile d'accent + branche ; ses couleurs recopient `theme.rs`, à changer ensemble).
- `src/assets.rs` + `assets/icons/` : 3 glyphes Hugeicons (MIT) repris de Monocode, embarqués (`include_bytes!`), crédités dans `NOTICE`.
- `examples/screenshot.rs` : rend la fenêtre hors écran en PNG (voir commandes).
- `crates/gwm-client/` : tout ce qui parle à gwm, sans dépendance GPUI (workspace Cargo).
  - `src/lib.rs` : types du contrat, `gwm list --format=json`, `gwm remove` derrière sa garde (`removable`), et `Repo::locate` (un seul `git rev-parse --show-toplevel --git-common-dir`).
  - `src/create.rs` : `gwm create` dans ses trois modes (`--issue`, triplet, `--name`), stdin fermé, jamais `--allow-bootstrap` ; `Create::created` retrouve le worktree créé dans une liste (issue ou branche, chemin nouveau), jamais dans la sortie texte de gwm.
  - `src/daemon.rs` : le flux `subscribe` (contrôle de `schema_version`, handshake de 5 s) et le chemin du socket par défaut de gwm.
  - `src/watch.rs` : la stratégie : daemon du socket par défaut s'il sert ce repo (toplevel **égal** à un de ses worktrees, jamais un préfixe : un sous-module n'est pas son parent), puis socket propre à l'app, puis `gwm daemon` lancé par l'app, puis un seul `gwm list`. Une sortie illisible du daemon d'un socket partagé passe au suivant ; sur le socket propre de l'app, c'est une dérive de contrat, affichée. Si git ne localise pas le repo (bare), un seul `gwm list` : c'est gwm qui juge ce qu'il sait lister.
  - `tests/real_daemon.rs` : contre un vrai `gwm daemon` sur des repos jetables. `tests/real_remove.rs` : `remove` contre un vrai `gwm`. `tests/real_create.rs` : `create` contre un vrai `gwm` (`--issue` seulement là où gwm refuse avant la forge).
- `src/theme.rs` : tokens : surfaces, opacités, rayons et tailles de Monocode Native (thème sombre, cadre rounded card), sauf l'opacité de la fenêtre, réglée sur ses captures (0.45 contre 0.85 par défaut chez lui, mesures dans le commentaire du token) ; accent (`ACCENT`, un seul token) et couleurs d'état de gwm (`claude-dark`).
- `.claude/rules/` : conventions path-scoped (`gpui-views.md`, `gwm-contract.md`).

## Conventions de code
- Rust édition 2024, 2 espaces, 120 colonnes (`rustfmt.toml`).
- La logique sans fenêtre se teste sous `#[cfg(test)]`, sur un échantillon réel de sortie gwm. Un changement de comportement arrive avec un test qui a échoué d'abord.
- L'UI se vérifie en lançant l'app, les tests ne voient pas un rendu cassé.

## Workflow Git
- Branche feature uniquement, jamais sur `main` directement, intégration dans `dev`.
- Worktree par défaut : `/me:issue-worktree-pr [desc]` (gwm). Variante checkout : `/me:issue-branch-pr [desc]`.
- Sprint : `/me:goal [desc]` (worktree autonome, merge progressif dans `dev`).
- Release : `/me:release [X.Y.Z]` — `/changelog` du repo → bump de `Cargo.toml` + `Cargo.lock`, commité sur `dev` → merge `dev` → `main` → tag **après** le merge → `gh release create --notes-file changelogs/<version>.md` (pas encore de CI de release). `changelogs/X.Y.Z.md`, sans `v`.
- Commits atomiques, Gitmoji + Conventional Commits, référencent l'issue.
- Issue remplie depuis `.github/ISSUE_TEMPLATE/*`, PR depuis `.github/PULL_REQUEST_TEMPLATE.md`.
- Review par défaut : boucle `/me:loop:claude-review-pr` (agent Claude frais, depuis le worktree). PR sensible : `/me:loop:codex-review-pr` (reviewer tiers). `/me:check-reviews [PR#]` à la main en second plan. `/me:loop:ci-until-green` avant merge.

## Recherche & outils
- Archi du repo : `graphify-out/graph.json` (rebuild gratuit : `graphify update .` **depuis la racine**, jamais depuis un worktree). L'interroger avant de grep à l'aveugle.
- Projet : mgrep → serena → find. Web : tavily / hyperbrowser → WebSearch. Docs : context7.
- Édition : `Edit`+`replace_all` / plusieurs `Edit` / `Write`.

## Note au merge de PR
- Vault : `~/Vault/perso/01 - Projects/gwm-gpui/decisions/` — arbitrage → `Template — Décision.md`, leçon → `Template — Retex.md`.
- Jamais : changelog, doc technique, tâche, procédure. Frontmatter quoté (cf. `AGENTS.md` du vault).

## Notes spécifiques
- 🔴 **gwm reste la source de vérité** : aucune logique de worktree ici. Pas de dépendance Cargo sur `gwm-cli` : son `[lib]` est un point d'accès interne pour ses tests, hors SemVer. Lectures par le JSON gelé (`docs/schema/` de gwm-cli) ou le daemon JSON-RPC, écritures par un subprocess `gwm`. Détail : `.claude/rules/gwm-contract.md`.
- Une fonctionnalité qui demande un changement de contrat se fait d'abord dans gwm-cli.
- Daemon : gwm le documente comme lancé par l'utilisateur, un par repo, sur un socket par défaut partagé. L'app le lance quand même s'il manque, sinon la liste ne serait pas live, sur `<base>/gwm-gpui-<fnv1a du common dir git>.sock`, `<base>` étant celle du socket par défaut de gwm (`$XDG_RUNTIME_DIR`, sinon `$TMPDIR`) : un par repo, quel que soit le worktree ouvert, et l'arrête au `on_app_quit`. Tuée au lieu de quittée, elle laisse le daemon tourner : le lancement suivant le réutilise sans le posséder, il tourne jusqu'à un arrêt à la main. Le daemon ne passe pas par le trust ledger de `.gwm.toml` : pas de `--allow-bootstrap`.
- Manque du contrat, gardé ici : pas de remove par chemin, et `gwm remove <motif>` retombe sur une recherche par sous-chaîne quand le nom ne résout rien (`auth` supprimé ailleurs → `auth-v2`). Juste avant de supprimer, `gwm_client::remove` relance `gwm list` et exige le nom seul sur le chemin sélectionné, id d'aucun autre, ni main ni le worktree où l'app a été ouverte (gwm et son daemon y tournent), et refuse si le worktree a des changements non commités alors que l'alerte confirmée le disait propre. Une seule suppression à la fois : gwm résout par nom, contre une liste qui peut encore montrer celle en cours.
- Manques du contrat, remontés et non contournés dans gwm-cli : aucune méthode ne nomme le repo d'un daemon (on compare les chemins du premier snapshot), aucune commande ne donne le socket par défaut résolu (la résolution est recopiée dans `daemon.rs`), `subscribe` n'a pas de réponse, le premier snapshot en tient lieu, et le flux n'a pas de heartbeat : un daemon suspendu (Ctrl-Z) ou dont les scans échouent en boucle garde la connexion ouverte sans rien envoyer, et la liste reste affichée « live » alors qu'elle est figée.
- `gpui-pre` vient de crates.io, **non modifié**. Monocode Native maintient une copie patchée de GPUI dans `vendor/` ; ce qui en dépend et manque ici (décision du user, ne pas vendorer sans lui) :
  - le matching de graisse (`vendor/gpui-pre-apple/src/text_system.rs` chez Monocode) : **vérifié**, `MEDIUM` rend au pixel près comme `NORMAL` dans SF, seul `SEMIBOLD` s'épaissit. Les titres de section sont donc en Regular ;
  - les fondus de bord (liste de la sidebar, libellé d'onglet ; `metal_renderer.rs`) : ici, des ellipses ;
  - le masque de coins de la carte : remplacé par des chemins remplis de wash (`card_corners`), comme Monocode hors macOS ;
  - le repositionnement Auto Layout des feux (`traffic_lights.rs` chez Monocode) : ici `traffic_light_position` seul.
  Images et détail : `docs/gpui-gaps.md`. Pas de manque, vérifié : le mélange alpha non patché donne bien du source-over sur les fonds empilés (alpha mesuré 0.859 = 0.85 + 0.05 × 0.15), et le `Blurred` de `gpui-pre-macos` 0.3.11 utilise déjà le matériau `UnderWindowBackground` (`src/window.rs:3947`) que Monocode a dû patcher sur sa 0.3.9. Le flou lui-même vient du window server et ne se voit pas hors écran : à juger en lançant l'app.
- Monocode est sous MIT : copier du code ou des assets impose son attribution dans `NOTICE`.
- Le panneau des fichiers modifiés de Monocode n'a pas d'équivalent : `gwm status --json` donne `branch`, `issue` et `pr`, aucune liste de fichiers. À ajouter dans gwm-cli d'abord.
- `.cargo/audit.toml` ignore 6 advisories `unmaintained` (aucune vulnérabilité), toutes transitives via GPUI : les sortir du graphe demanderait de forker GPUI. **À réexaminer à chaque bump de `gpui-pre`** : `cargo tree -i <crate> --target all -e normal,build` par ID, et retirer ceux qui ne sortent plus. Jamais d'ID ajouté sans sa raison et son chemin, et la CI garde `--deny warnings`.
- `runtime_shaders` : les shaders Metal compilent au lancement, il suffit des Command Line Tools.
- Pas de hot reload avec GPUI : on relance.
- `main` n'est pas protégée pour l'instant, à durcir comme gwm-cli quand le projet se stabilise.
