#!/usr/bin/env bash
set -euo pipefail

COMMAND="${1:-activate}"
if [[ $# -gt 0 ]]; then shift; fi
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONFIG_PATH="$REPO_ROOT/.cortex/config.json"
ENGINE_PYTHON='C:\Users\jacks\OneDrive\Desktop\pulseflow-governor\tools\cortex\.venv\Scripts\python.exe'
ENGINE_MODULE_ROOT='C:\Users\jacks\OneDrive\Desktop\pulseflow-governor\tools\cortex'
CORTEX_HOME_PATH='C:\Users\jacks\.cortex'

if [[ ! -f "$CONFIG_PATH" ]]; then
  echo "Cortex config is missing: $CONFIG_PATH. Re-run repository bootstrap." >&2
  exit 2
fi

if [[ -z "$ENGINE_PYTHON" && -n "${CORTEX_PYTHON:-}" ]]; then ENGINE_PYTHON="$CORTEX_PYTHON"; fi
if [[ -z "$CORTEX_HOME_PATH" && -n "${CORTEX_HOME:-}" ]]; then CORTEX_HOME_PATH="$CORTEX_HOME"; fi
if [[ -d "$ENGINE_MODULE_ROOT" ]]; then export PYTHONPATH="$ENGINE_MODULE_ROOT${PYTHONPATH:+:$PYTHONPATH}"; fi
if [[ "$ENGINE_PYTHON" == */* && ! -x "$ENGINE_PYTHON" ]]; then ENGINE_PYTHON=""; fi
if [[ -z "$ENGINE_PYTHON" ]] && command -v python3 >/dev/null 2>&1; then ENGINE_PYTHON="$(command -v python3)"; fi
if [[ -z "$ENGINE_PYTHON" ]] && command -v python >/dev/null 2>&1; then ENGINE_PYTHON="$(command -v python)"; fi
if [[ -z "$ENGINE_PYTHON" ]]; then
  echo "Cortex Python was not found. Set CORTEX_PYTHON or re-run repository bootstrap." >&2
  exit 2
fi
if ! "$ENGINE_PYTHON" -c 'import cortex' >/dev/null 2>&1; then
  echo "The selected Python cannot import Cortex. Set CORTEX_PYTHON or re-run repository bootstrap." >&2
  exit 2
fi

REPO_NAME="$("$ENGINE_PYTHON" -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["repository_name"])' "$CONFIG_PATH")"

case "$COMMAND" in
  activate)
    TASK=""
    BUDGET="1200"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --task) TASK="${2:-}"; shift 2 ;;
        --budget) BUDGET="${2:-1200}"; shift 2 ;;
        *) echo "Unknown activate argument: $1" >&2; exit 2 ;;
      esac
    done
    [[ -n "$TASK" ]] || { echo "--task is required" >&2; exit 2; }
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" activate --repo "$REPO_NAME" --task "$TASK" --budget "$BUDGET" --json
    ;;
  bootstrap)
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" bootstrap "$REPO_ROOT" --name "$REPO_NAME" --json
    ;;
  query)
    QUERY=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --query) QUERY="${2:-}"; shift 2 ;;
        *) echo "Unknown query argument: $1" >&2; exit 2 ;;
      esac
    done
    [[ -n "$QUERY" ]] || { echo "--query is required" >&2; exit 2; }
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" query "$QUERY" --repo "$REPO_NAME" --json
    ;;
  remember)
    KIND="discovery"
    TEXT=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --kind) KIND="${2:-discovery}"; shift 2 ;;
        --text) TEXT="${2:-}"; shift 2 ;;
        *) echo "Unknown remember argument: $1" >&2; exit 2 ;;
      esac
    done
    [[ -n "$TEXT" ]] || { echo "--text is required" >&2; exit 2; }
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" remember --repo "$REPO_NAME" --kind "$KIND" --text "$TEXT" --json
    ;;
  interlink)
    TASK=""
    LEARN=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --task) TASK="${2:-}"; shift 2 ;;
        --learn) LEARN="--learn"; shift ;;
        *) echo "Unknown interlink argument: $1" >&2; exit 2 ;;
      esac
    done
    [[ -n "$TASK" ]] || { echo "--task is required" >&2; exit 2; }
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" interlink --repo "$REPO_NAME" --task "$TASK" ${LEARN:+$LEARN} --json
    ;;
  thalamus)
    TASK=""
    BUDGET="1200"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --task) TASK="${2:-}"; shift 2 ;;
        --budget) BUDGET="${2:-1200}"; shift 2 ;;
        *) echo "Unknown thalamus argument: $1" >&2; exit 2 ;;
      esac
    done
    [[ -n "$TASK" ]] || { echo "--task is required" >&2; exit 2; }
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" thalamus --repo "$REPO_NAME" --task "$TASK" --budget "$BUDGET" --json
    ;;
  consolidate|verify|status|graph|telemetry|environment|neural-replay|doctor)
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" "$COMMAND" --repo "$REPO_NAME" --json
    ;;
  *) echo "Unknown command: $COMMAND" >&2; exit 2 ;;
esac
