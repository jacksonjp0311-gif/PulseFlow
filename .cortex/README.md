# Repository Cortex Integration

This directory is the repository-local integration surface for the installed Cortex engine.

- `config.json` identifies this repository, records the Cortex Python interpreter/module location, and controls assimilation.
- `bootstrap_certificate.json` records the latest verified inventory and coverage state.
- `bin/cortex.ps1` and `bin/cortex.sh` are stable entry points for Codex and other agents.
- `runtime/` contains generated context and learned-environment packets and is intentionally ignored by Git.

Cortex's global database normally lives at `~/.cortex/cortex.db`. The neural interlink shares that database and never creates a competing memory authority. Repository source remains authoritative.
