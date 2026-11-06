---
description: "Génère le changelog du projet (extract → rédaction → split → metrics), rangé dans changelogs/"
argument-hint: "X.Y.Z [--entries=verbose|normal|short] [--version=verbose|normal|short|null] [--base=<branch>] [--compare=<branch>]"
allowed-tools: Bash(changelog-generator:*), Bash(git :*), Read, Write, Edit
---

# /changelog — gwm-gpui

Config par défaut (gravée, surchargeable par flags) :
- Entrées : verbose · Détails version : normal · Client : non
- Base : main → Compare : dev · Sortie : ./changelogs

Arguments : `$ARGUMENTS` (1er = version cible `X.Y.Z` ; flags optionnels d'override).

## Étapes
1. **Extract** : `~/.claude/skills/changelog-generator/bin/changelog-generator extract --base main --compare dev` → `git-log.txt`.
2. **Rédaction** (toi) : lire `CHANGELOG.md` + `git-log.txt`, rédiger la section de la version cible au format Keep a Changelog (`## [X.Y.Z] - date`, en anglais), en respectant la verbosité retenue :
   - Entrées `verbose` : titre + réf `#issue` / `[#pr]` + contexte technique et choix d'implémentation.
   - En-tête de version `normal` : résumé court (1-2 phrases).
   - Catégoriser en Added / Changed / Fixed / Security, référencer `#issue` / `[#pr]`.
3. **Split** : `changelog-generator split --output-dir ./changelogs --clean` → `changelogs/X.Y.Z.md` (sans préfixe `v` ; pré-release → `pre-releases/`).
4. **Racine** : `CHANGELOG.md` ne garde que `## [Unreleased]` et l'index `## Past releases`.

## Règles
- Evidence-based : se baser sur `git-log.txt` réel, ne rien inventer.
- Ne pas réécrire les versions passées.
- S'inscrit dans le workflow Release : ce `/changelog` est l'étape 1 (cf. `/me:release`).
