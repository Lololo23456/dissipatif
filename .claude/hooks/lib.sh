#!/usr/bin/env bash
# Lit un champ du JSON reçu sur l'entrée standard. Utilise jq, sinon python3.
json_field() {
  local input="$1" path="$2"
  if command -v jq >/dev/null 2>&1; then
    printf '%s' "$input" | jq -r "$path // empty"
  elif command -v python3 >/dev/null 2>&1; then
    printf '%s' "$input" | python3 -c '
import sys, json
d = json.load(sys.stdin)
for k in sys.argv[1].lstrip(".").split("."):
    d = d.get(k) if isinstance(d, dict) else None
print("" if d is None else (str(d).lower() if isinstance(d, bool) else d))
' "$path"
  fi
}
