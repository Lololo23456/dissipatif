#!/usr/bin/env bash
# PreToolUse : protège les fichiers qu'on ne doit pas modifier sans le demander.
source "$CLAUDE_PROJECT_DIR/.claude/hooks/lib.sh"
INPUT=$(cat)
FILE=$(json_field "$INPUT" ".tool_input.file_path")
[ -z "$FILE" ] && exit 0
case "$FILE" in
  *.env|*/.env.*)
    echo "Modification de .env interdite." >&2; exit 2 ;;
  */Cargo.lock)
    echo "Ne modifie pas Cargo.lock à la main : passe par cargo." >&2; exit 2 ;;
  *.png|*.jpg|*.jpeg|*.gltf|*.glb)
    echo "Le projet n'utilise aucun asset dessiné ou importé : tout l'art est procédural (voir .claude/rules/rendu.md)." >&2; exit 2 ;;
esac
exit 0
