# INTERNAL CORTEX - Repository Integration

This directory is the explicitly labeled, repository-local integration surface for the installed Cortex engine. It is not a second source of truth and it is not the host repository's application code.

- `config.json` identifies this surface as `internal_repository_cortex`, identifies the host repository, records the Cortex Python interpreter/module location, and controls assimilation.
- `bootstrap_certificate.json` records the latest verified inventory and coverage state.
- `bin/cortex.ps1` and `bin/cortex.sh` are stable entry points for Codex and other agents.
- `runtime/` contains generated context and learned-environment packets and is intentionally ignored by Git.

## Stable CORTEX_HOME (production)

Cross-process identity and memory continuity require a durable home, not a temp directory.

- Prefer a fixed path such as `~/.cortex` (or another long-lived directory you control).
- Avoid binding `cortex_home` in `config.json` to OS temp paths (`%TEMP%`, `/tmp`, `TemporaryDirectory`, CI scratch).
- If `config.json` points at a temp home, re-bootstrap with an explicit stable home:

```powershell
$env:CORTEX_HOME = Join-Path $env:USERPROFILE ".cortex"
python -m cortex bootstrap . --name YourRepo --json
```

```bash
export CORTEX_HOME="$HOME/.cortex"
python -m cortex bootstrap . --name YourRepo --json
```

Then verify with the wrapper:

```powershell
.\bin\cortex.ps1 identity
```

```bash
./bin/cortex.sh identity
```

Cortex's global database normally lives at `~/.cortex/cortex.db`. The neural interlink shares that database and never creates a competing memory authority. Repository source remains authoritative.

### Operator commands on the wrapper

Beyond activate/query/remember, the installed wrappers also expose: `identity`, `distill`, `kernels`, `interconnect`, `immune`, `metrics`, `prune`, `organism`, `breathe`, and `causal` (including `causal probe` for matched recall pairs).
