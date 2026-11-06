# gwm-gpui

App desktop native pour [gwm](https://github.com/kbrdn1/gwm-cli), en Rust sur GPUI (`gpui-pre`, snapshots de Zed). macOS uniquement pour l'instant.

## Stack & commandes
⚠️ `export PATH="$HOME/.cargo/bin:$PATH"` d'abord : un rustc 1.89 du profil nix peut passer avant rustup, qui lui suit `rust-toolchain.toml` (1.99.1 ; `gpui-pre` 0.3.11 exige ≥ 1.98).

- Build : `cargo build`
- Test  : `cargo test`
- Lint/Format : `cargo clippy --all-targets -- -D warnings` · `cargo fmt --all`
- Run   : `cargo run -- <chemin-du-repo>` (par défaut le dossier courant). `gwm` doit être sur le `PATH`.

Premier build long (GPUI compile tout) : `nice -n 10 cargo build -j 4` pour ne pas saturer la machine.

## Structure
- `src/main.rs` : bootstrap de l'application GPUI et `WorktreesView`, la seule vue.
- `src/worktrees.rs` : lecture du contrat gwm (`gwm list --format=json`), types et parsing testés.
- `src/theme.rs` : tokens de couleur, alignés sur le preset `claude-dark` de gwm-cli.
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
- `gpui-pre` vient de crates.io, **non modifié** : pas de flou d'arrière-plan ni de fondus comme Monocode Native, qui maintient une copie patchée de GPUI dans `vendor/`. Monocode est sous MIT : copier du code impose son attribution dans `NOTICE`.
- `runtime_shaders` : les shaders Metal compilent au lancement, il suffit des Command Line Tools.
- Pas de hot reload avec GPUI : on relance.
- `main` n'est pas protégée pour l'instant, à durcir comme gwm-cli quand le projet se stabilise.
