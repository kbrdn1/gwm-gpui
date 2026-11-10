---
paths:
  - "src/**/*.rs"
---
# Vues GPUI — conventions

- Jamais d'appel bloquant dans `render` ni sur le thread UI : un subprocess ou une I/O ponctuels passent par `cx.background_spawn`, puis `this.update(cx, …)` et `cx.notify()`.
- Un flux qui bloque toute la vie de l'app (`gwm_client::watch` sur le socket du daemon) a son propre thread et passe ses événements à la vue par un `async-channel` : sur l'exécuteur de fond, il immobiliserait un worker pour de bon. Modèle : `WorktreesView::new` dans `src/main.rs`.
- `render` ne fait que lire l'état. Un changement d'état sans `cx.notify()` ne repeint rien, sans aucune erreur.
- Une couleur vit dans `src/theme.rs`, jamais en hex dans une vue. Les tokens suivent le preset `claude-dark` de gwm-cli.
- Un fichier par vue, découpé tôt. Monocode Native a laissé `shell.rs` monter à 13 437 lignes sur 197 507 : c'est le piège à éviter ici.
- `gpui` et `gpui_platform` bougent ensemble, épinglés en `=`. Un bump est une PR à part, testée en lançant l'app.
- Ne jamais activer la feature `screen-capture` : elle tire `zed-scap`, que Monocode refuse dans son graphe de dépendances parce que sa licence publiée ajoute des restrictions à ce que ses métadonnées annoncent (MIT).
- Lancer l'app avant de déclarer un changement d'UI terminé : les tests ne voient pas un rendu cassé.
