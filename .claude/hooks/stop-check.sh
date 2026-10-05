#!/usr/bin/env bash
# Stop : vérifie que le workspace compile avant que Claude rende la main.
# Si la compilation échoue, code 2 : Claude reçoit l'erreur et continue pour la corriger.
source "$CLAUDE_PROJECT_DIR/.claude/hooks/lib.sh"
INPUT=$(cat)
# Évite une boucle infinie si Claude est déjà en train de corriger suite à ce hook.
[ "$(json_field "$INPUT" ".stop_hook_active")" = "true" ] && exit 0
cd "$CLAUDE_PROJECT_DIR" || exit 0
[ -f Cargo.toml ] || exit 0
command -v cargo >/dev/null 2>&1 || exit 0
# Rien de modifié dans le code depuis le dernier commit : inutile de vérifier.
if command -v git >/dev/null 2>&1 && git rev-parse --git-dir >/dev/null 2>&1; then
  git status --porcelain -- '*.rs' '*.wgsl' 'Cargo.toml' '**/Cargo.toml' | grep -q . || exit 0
fi
if ! OUT=$(cargo check --workspace --all-targets --quiet 2>&1); then
  echo "cargo check échoue. Corrige avant de terminer :" >&2
  echo "$OUT" | grep -E '^(error|warning: unused)' -A 6 | head -n 60 >&2
  exit 2
fi
exit 0
