# PulseFlow application icon

The icon distills the dashboard's pulse-feedback modulation core into a compact
mark: concentric governor rings, four workload channels, verified green flow,
and one amber observation trace.

## Distributed assets

- `assets/icons/pulseflow-governor.ico` contains 16, 24, 32, 48, 64, 128,
  and 256 pixel Windows icon frames.
- PNG files are provided at 16, 24, 32, 48, 64, 128, 192, 256, 512, and
  high-resolution source sizes.
- `/favicon.ico`, PNG icon routes, and `/site.webmanifest` are embedded in the
  release binary for browser and installable-web-app use.

## Windows installation

Run (or re-run after every evolution):

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\Install-PulseFlow.ps1 -DesktopShortcut
```

The installer:

1. builds the release binary (omit with `-SkipBuild` if `target\release` is fresh);
2. stops any running `pulseflow-governor` so the exe can be overwritten;
3. copies the binary, config, and ICO into the per-user install directory;
4. rewrites Start Menu and Desktop shortcuts with a versioned description;
5. records `installation.json` with the version from `Cargo.toml`.

If a desktop shortcut already exists (or a prior install had `-DesktopShortcut`),
later installs refresh that desktop icon automatically even without the switch.

Installed shortcuts use `Launch-PulseFlow.ps1`, which reuses a healthy instance
when one already owns port 8791. Otherwise it starts the governor, waits for its
health endpoint, and opens the dashboard with a version-stamped URL so the tab
title does not stay stuck on an old build.

The release executable embeds the same multi-resolution icon during the Rust
build, so Windows Explorer keeps the PulseFlow identity even when the
executable is copied outside the installed shortcut.

For isolated packaging or verification, `-InstallDirectory` and
`-ShortcutDirectory` can redirect both destinations.

## Generation record

The icon was generated with the built-in image generation tool using
`docs/PULSEFLOW-UI-CONCEPT.png` as a visual reference. The request specified a
centered engineering emblem derived from the modulation core, with concentric
authority rings, four channel spokes, charcoal structure, luminous PulseFlow
green, one amber observation accent, no text, and no dashboard chrome. A flat
magenta key background was removed locally, then ImageMagick produced the PNG
and multi-resolution ICO variants.
