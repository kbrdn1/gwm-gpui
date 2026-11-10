---
paths:
  - "src/**/*.rs"
---
# Vues GPUI — conventions

- Jamais d'appel bloquant dans `render` ni sur le thread UI : un subprocess ou une I/O ponctuels passent par `cx.background_spawn`, puis `this.update(cx, …)` et `cx.notify()`.
- Un flux qui bloque toute la vie de l'app (`gwm_client::watch` sur le socket du daemon) a son propre thread et passe ses événements à la vue par un `async-channel` : sur l'exécuteur de fond, il immobiliserait un worker pour de bon. Modèle : `Workspace::new` dans `src/workspace.rs`.
- `render` ne fait que lire l'état. Un changement d'état sans `cx.notify()` ne repeint rien, sans aucune erreur.
- Une couleur vit dans `src/theme.rs`, jamais dans une vue (ni hex, ni `hsla`, ni `white()`). Surfaces, opacités, rayons et tailles suivent Monocode Native ; l'accent (un seul token) et les couleurs d'état suivent le preset `claude-dark` de gwm-cli.
- Pas de bouton sans fonction : un chevron de menu, une recherche ou des flèches de navigation qui ne font rien sont des données inventées.
- Un fichier par vue, découpé tôt. Monocode Native a laissé `shell.rs` monter à 13 437 lignes sur 197 507 : c'est le piège à éviter ici.
- `gpui` et `gpui_platform` bougent ensemble, épinglés en `=`. Un bump est une PR à part, testée en lançant l'app.
- Ne jamais activer la feature `screen-capture` : elle tire `zed-scap`, que Monocode refuse dans son graphe de dépendances parce que sa licence publiée ajoute des restrictions à ce que ses métadonnées annoncent (MIT). Depuis la feature `screenshot`, `zed-scap` figure dans `Cargo.lock` (la feature faible `x11 = ["scap?/x11"]` de `test-support` suffit à l'y inscrire) mais n'est compilé pour aucune cible : `cargo tree -i zed-scap --target all --all-features` doit rester vide.
- Lancer l'app avant de déclarer un changement d'UI terminé : les tests ne voient pas un rendu cassé. Et regarder le rendu : `examples/screenshot.rs` produit un PNG hors écran (voir `CLAUDE.md`).
- Un élément `absolute()` sans `top`/`left` garde sa position statique, après ses frères : ancrer avec `top_0().left_0()`.
