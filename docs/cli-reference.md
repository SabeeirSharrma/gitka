# CLI Command Reference

Below is a complete list of Gitka commands and their descriptions.

## Setup

| Command | Description |
|---|---|
| `gitka wipe --target <dev>` | Wipe removable drive + set up (SAFETY: removable only, requires YES); copies CLI + GUI to USB |
| `gitka wipe --source github --username <user> --token <pat>` | Wipe + immediately configure GitHub source |
| `gitka init` | Initialize a new Gitka backup on existing storage |
| `gitka init --volume-size <MB>` | Initialize with volume splitting (e.g., 4096 for FAT32) |
| `gitka init --dedup` | Initialize with cross-repo deduplication |
| `gitka init --source github --token <pat>` | Initialize + configure GitHub source |
| `gitka init -i` | Run the interactive setup wizard |

## Authentication

| Command | Description |
|---|---|
| `gitka auth --token <pat>` | Verify a GitHub PAT and store it in config |
| `gitka auth --status` | Show current GitHub authentication status |
| `gitka auth --status --verify` | Verify the stored token against the GitHub API |
| `gitka auth --status --json` | JSON output for the GUI |

You can also pass the token inline during init/wipe:
`gitka init --token <pat>` or `gitka wipe --token <pat>`.

## Discovery & Sync

| Command | Description |
|---|---|
| `gitka scan` | Discover repos from source, show budget check |
| `gitka sync` | Clone/fetch repos (incremental — skips untouched repos) |
| `gitka sync --repos a,b` | Sync only the named repos |
| `gitka sync` (with divergence) | Auto-merges when conflicts are absent; flags when they exist |

## Inspection

| Command | Description |
|---|---|
| `gitka status` | Per-repo state, archive sizes, active sessions |
| `gitka status --json` | Same output as JSON (for GUI/automation) |
| `gitka usb` | List detected removable drives |
| `gitka usb --json` | Drive list as JSON (for GUI/automation) |

## Extract & Lock

| Command | Description |
|---|---|
| `gitka unlock <repo>` | Extract for offline commit access (budget-aware) |
| `gitka lock <repo>` | Recompress + close session (shows audit trail) |
| `gitka serve <repo>` | Serve over LAN via GitFlare |
| `gitka serve <repo> --stop` | Stop the LAN server, recompress, clear extraction |
| `gitka verify` | Manual integrity + permissions check |
| `gitka verify --repos a,b` | Verify specific repos only |
| `gitka verify -v` | Verbose output |
| `gitka repair <repo>` | Use recovery records to fix a corrupted repo |

## Configuration & Tools

| Command | Description |
|---|---|
| `gitka config` | Show full configuration |
| `gitka config --get key` | Get a config value (e.g., `compression.dedup`) |
| `gitka config --set key=value` | Set a config value (e.g., `toggles.encryption=true`) |
| `gitka import <path>` | Import a local git repo into backup |
| `gitka import <path> --name <name>` | Import with a custom name |
| `gitka train-dict` | Train zstd dictionary from existing archives |
| `gitka train-dict --source <dir>` | Train from a specific sample directory |
| `gitka gui` | Launch the GUI |

You can also run `gitka` with no subcommand — it launches the GUI, same as `gitka gui`.
