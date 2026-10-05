#!/usr/bin/env bash
# PostToolUse : formate les fichiers Rust et valide les shaders WGSL modifiés.
# Code de sortie 2 = le message sur stderr est renvoyé à Claude pour qu'il corrige.
source "$CLAUDE_PROJECT_DIR/.claude/hooks/lib.sh"
INPUT=$(cat)
FILE=$(json_field "$INPUT" ".tool_input.file_path")
[ -z "$FILE" ] || [ ! -f "$FILE" ] && exit 0

case "$FILE" in
  *.rs)
    if command -v rustfmt >/dev/null 2>&1; then
      rustfmt --edition 2024 "$FILE" 2>/dev/null || true
    fi
    ;;
  *.wgsl)
    if command -v naga >/dev/null 2>&1; then
      if ! OUT=$(naga "$FILE" 2>&1); then
        echo "Shader WGSL invalide ($FILE) :" >&2
        echo "$OUT" | head -n 40 >&2
        exit 2
      fi
    else
      echo "naga n'est pas installé : shader non validé. Installer avec : cargo install naga-cli --version 30.0.1" >&2
    fi
    ;;
esac
exit 0
