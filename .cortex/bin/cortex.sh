#!/usr/bin/env bash
set -euo pipefail

COMMAND="${1:-activate}"
if [[ $# -gt 0 ]]; then shift; fi
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CONFIG_PATH="$REPO_ROOT/.cortex/config.json"
ENGINE_PYTHON='C:\Program Files\Python312\python.exe'
ENGINE_MODULE_ROOT='C:\Users\jacks\OneDrive\Desktop\Cortex'
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
    BUDGET="800"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --task) TASK="${2:-}"; shift 2 ;;
        --budget) BUDGET="${2:-800}"; shift 2 ;;
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
  ritual)
    TASK=""
    KIND="discovery"
    TEXT=""
    BUDGET="800"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --task) TASK="${2:-}"; shift 2 ;;
        --kind) KIND="${2:-discovery}"; shift 2 ;;
        --text) TEXT="${2:-}"; shift 2 ;;
        --budget) BUDGET="${2:-800}"; shift 2 ;;
        *) echo "Unknown ritual argument: $1" >&2; exit 2 ;;
      esac
    done
    [[ -n "$TASK" ]] || { echo "--task is required" >&2; exit 2; }
    if [[ -n "$TEXT" ]]; then
      exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" ritual --repo "$REPO_NAME" --task "$TASK" --budget "$BUDGET" --remember-kind "$KIND" --remember-text "$TEXT" --json
    else
      exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" ritual --repo "$REPO_NAME" --task "$TASK" --budget "$BUDGET" --json
    fi
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
    BUDGET="800"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --task) TASK="${2:-}"; shift 2 ;;
        --budget) BUDGET="${2:-800}"; shift 2 ;;
        *) echo "Unknown thalamus argument: $1" >&2; exit 2 ;;
      esac
    done
    [[ -n "$TASK" ]] || { echo "--task is required" >&2; exit 2; }
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" thalamus --repo "$REPO_NAME" --task "$TASK" --budget "$BUDGET" --json
    ;;
  identity)
    REPO_ARG=()
    PATH_ARG=()
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --repo) REPO_ARG=(--repo "${2:-}"); shift 2 ;;
        --path) PATH_ARG=(--path "${2:-}"); shift 2 ;;
        *) echo "Unknown identity argument: $1" >&2; exit 2 ;;
      esac
    done
    if [[ ${#REPO_ARG[@]} -eq 0 ]]; then REPO_ARG=(--repo "$REPO_NAME"); fi
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" identity "${REPO_ARG[@]}" "${PATH_ARG[@]}" --json
    ;;
  distill)
    EXTRA=()
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --no-seal) EXTRA+=(--no-seal); shift ;;
        --doctrine-only) EXTRA+=(--doctrine-only); shift ;;
        *) echo "Unknown distill argument: $1" >&2; exit 2 ;;
      esac
    done
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" distill --repo "$REPO_NAME" "${EXTRA[@]}" --json
    ;;
  kernels)
    EXTRA=()
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --annotate) EXTRA+=(--annotate); shift ;;
        *) echo "Unknown kernels argument: $1" >&2; exit 2 ;;
      esac
    done
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" kernels --repo "$REPO_NAME" "${EXTRA[@]}" --json
    ;;
  prune)
    EXTRA=()
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --dry-run) EXTRA+=(--dry-run); shift ;;
        --decay) EXTRA+=(--decay); shift ;;
        *) echo "Unknown prune argument: $1" >&2; exit 2 ;;
      esac
    done
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" prune --repo "$REPO_NAME" "${EXTRA[@]}" --json
    ;;
  organism)
    TASK=""
    BUDGET="800"
    PROFILE="agent"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --task) TASK="${2:-}"; shift 2 ;;
        --budget) BUDGET="${2:-800}"; shift 2 ;;
        --profile) PROFILE="${2:-agent}"; shift 2 ;;
        *) echo "Unknown organism argument: $1" >&2; exit 2 ;;
      esac
    done
    [[ -n "$TASK" ]] || { echo "--task is required" >&2; exit 2; }
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" organism --repo "$REPO_NAME" --task "$TASK" --budget "$BUDGET" --profile "$PROFILE" --json
    ;;
  breathe)
    TASK=""
    BUDGET="800"
    PROFILE="agent"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --task) TASK="${2:-}"; shift 2 ;;
        --budget) BUDGET="${2:-800}"; shift 2 ;;
        --profile) PROFILE="${2:-agent}"; shift 2 ;;
        *) echo "Unknown breathe argument: $1" >&2; exit 2 ;;
      esac
    done
    if [[ -n "$TASK" ]]; then
      exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" breathe --repo "$REPO_NAME" --task "$TASK" --budget "$BUDGET" --profile "$PROFILE" --json
    else
      exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" breathe --repo "$REPO_NAME" --budget "$BUDGET" --profile "$PROFILE" --json
    fi
    ;;
  causal)
    ACTION="${1:-status}"
    if [[ $# -gt 0 ]]; then shift; fi
    case "$ACTION" in
      status|report|evaluate)
        exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" causal "$ACTION" --repo "$REPO_NAME" --json
        ;;
      probe)
        TASK=""
        SLOT="before"
        K="8"
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --task) TASK="${2:-}"; shift 2 ;;
            --query) TASK="${2:-}"; shift 2 ;;
            --slot) SLOT="${2:-before}"; shift 2 ;;
            --k) K="${2:-8}"; shift 2 ;;
            *) echo "Unknown causal probe argument: $1" >&2; exit 2 ;;
          esac
        done
        [[ -n "$TASK" ]] || { echo "--task is required for causal probe" >&2; exit 2; }
        exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" causal probe --repo "$REPO_NAME" --task "$TASK" --slot "$SLOT" --k "$K" --json
        ;;
      *) echo "Unknown causal action: $ACTION (status|report|evaluate|probe)" >&2; exit 2 ;;
    esac
    ;;
  glyphs)
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" glyphs --json
    ;;
  stream)
    SACTION="${1:-status}"
    if [[ $# -gt 0 ]]; then shift; fi
    case "$SACTION" in
      status|seal)
        exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" stream "$SACTION" --repo "$REPO_NAME" --json
        ;;
      *) echo "Unknown stream action: $SACTION (status|seal)" >&2; exit 2 ;;
    esac
    ;;
  harness)
    BUDGET="500"
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --budget) BUDGET="${2:-500}"; shift 2 ;;
        *) echo "Unknown harness argument: $1" >&2; exit 2 ;;
      esac
    done
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" harness --repo "$REPO_NAME" --budget "$BUDGET" --json
    ;;
  hygiene)
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" hygiene --repo "$REPO_NAME" --json
    ;;
  evolve)
    ACT=""
    STATUS="verified"
    VERIFICATION="test"
    TASK=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --activation-id) ACT="${2:-}"; shift 2 ;;
        --status) STATUS="${2:-verified}"; shift 2 ;;
        --verification) VERIFICATION="${2:-test}"; shift 2 ;;
        --task) TASK="${2:-}"; shift 2 ;;
        *) echo "Unknown evolve argument: $1" >&2; exit 2 ;;
      esac
    done
    [[ -n "$ACT" ]] || { echo "--activation-id is required" >&2; exit 2; }
    if [[ -n "$TASK" ]]; then
      exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" evolve --repo "$REPO_NAME" --activation-id "$ACT" --status "$STATUS" --verification "$VERIFICATION" --task "$TASK" --json
    else
      exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" evolve --repo "$REPO_NAME" --activation-id "$ACT" --status "$STATUS" --verification "$VERIFICATION" --json
    fi
    ;;
  consolidate|verify|status|graph|telemetry|environment|meta-language|neural-replay|doctor|interconnect|immune|metrics)
    exec "$ENGINE_PYTHON" -m cortex --home "$CORTEX_HOME_PATH" "$COMMAND" --repo "$REPO_NAME" --json
    ;;
  *) echo "Unknown command: $COMMAND" >&2; exit 2 ;;
esac
