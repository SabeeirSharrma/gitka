# Gitka Documentation

Welcome to the Gitka documentation! Gitka is a Ventoy-inspired tool that creates compressed, physical-media (USB/CD) local copies of all your GitHub/GitFlare repos — with offline commit capability and LAN-based sharing.

## What is Gitka?

Gitka keeps all your repos compressed on removable media. When you need to work on one, it extracts temporarily, lets you commit offline, then recompresses when you're done.

### Key Features
- **Aggressive compression** — auto-selects zstd tier based on available space
- **Offline commits** — extract a repo, commit locally, sync later
- **LAN sharing** — serve a repo to others on your network via GitFlare
- **Recovery records** — optional par2 redundancy for corruption protection
- **Crash resilience** — detects orphaned extractions and recompresses automatically
- **Volume splitting** — split archives into fixed-size parts for CD/DVD or FAT32 limits
- **Cross-repo dedup** — share common blobs across repos to save space
- **Auto-merge** — divergent branches with no conflicts merge on `gitka sync`
- **Default branch detection** — works for `main`, `master`, `trunk`, or custom defaults
- **Host-side extraction** — bypass USB space pressure by extracting to host temp dir
- **Cross-platform GUI** — Tauri-based desktop app, fully offline, mirrors every CLI command

## Getting Started
- Check out the [Installation Guide](installation.md)
- Learn basic usage in the [Usage Guide](usage.md)
- Explore [Advanced Features](features.md)
- View the [Configuration](configuration.md) options
- See the full [CLI Command Reference](cli-reference.md)
