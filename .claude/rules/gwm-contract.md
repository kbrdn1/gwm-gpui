---
paths:
  - "crates/gwm-client/**"
---
# Contrat avec gwm — conventions

- Aucune dépendance Cargo sur `gwm-cli`, ni `path` ni `git`. Son `[lib]` est `#![doc(hidden)]`, c'est un point d'accès interne pour ses tests, hors SemVer (`docs/6.development/3.stability.md` dans gwm-cli).
- Lectures : le JSON de `docs/schema/*.schema.json`, via `gwm <cmd> --format=json` ou le daemon (`list`, `doctor`, `path`, `subscribe`). Les deux sont identiques à l'octet, donc passer de l'un à l'autre ne change pas les types.
- Écritures (`create`, `remove`, `review`) : un subprocess `gwm`. Aucune logique de worktree ici, gwm reste la source de vérité.
- Ne déclarer que les champs lus, et jamais `deny_unknown_fields` : le contrat ajoute des champs optionnels sans bump de version et demande aux consommateurs de les ignorer.
- Un champ du tier expérimental (`agents`) se lit à part de sa ligne : sa forme peut changer sans bump de version, et elle ne doit pas faire échouer les champs stables. Absent et illisible restent distincts (`Option<Result<…, String>>`), et l'illisible est dit là où le champ s'afficherait. Modèle : `agents::lenient`.
- Détecter une dérive : `params.schema_version` sur `worktrees.changed` côté daemon, `gwm --version` côté CLI.
- Une sortie illisible est une erreur affichée à l'utilisateur, jamais une liste vide.
- Tester `parse` sur un échantillon **réel** de sortie gwm, collé tel quel, pas sur un JSON réduit à ce qu'on lit.
