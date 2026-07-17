# Gitka

A Ventoy-inspired tool that creates compressed, physical-media (USB/CD) local copies of all your GitHub/GitFlare repos — with offline commit capability and LAN-based sharing.

## Install

```bash
curl -sSf https://raw.githubusercontent.com/SabeeirSharrma/gitka/refs/heads/main/install.sh | bash
```

This builds Gitka from source and installs it to `/usr/local/bin`.

## Quick Start

```bash
# Wipe a fresh USB drive and set up Gitka (interactive, safe)
gitka wipe --target /dev/sdb1 --username <github-username>

# Or initialize without wiping
gitka init --target /mnt/usb --username <github-username>

# Discover repos and check budget
gitka scan

# Clone/pull all repos
gitka sync

# Check what's going on
gitka status
```

## Usage

```bash
gitka wipe --target <device>         # Wipe removable drive + set up (REMOVABLE ONLY)
gitka init --target <path>           # Initialize without wiping
gitka scan                           # Discover repos, show budget
gitka sync                           # Clone/fetch repos from source
gitka status                         # Per-repo state + active sessions
gitka unlock <repo>                  # Extract for offline work
gitka lock <repo>                    # Recompress + close session
gitka serve <repo>                   # Serve over LAN via GitFlare
gitka serve <repo> --stop            # Stop the server
gitka verify                         # Check archive integrity
gitka repair <repo>                  # Fix corrupted repo with recovery records
gitka config                         # View/edit config
```

## How It Works

Gitka keeps all your repos compressed on removable media. When you need to work on one, it extracts temporarily, lets you commit offline, then recompresses when you're done.

- **Aggressive compression** — auto-selects zstd tier based on available space
- **Offline commits** — extract a repo, commit locally, sync later
- **LAN sharing** — serve a repo to others on your network via GitFlare
- **Recovery records** — optional par2 redundancy for corruption protection
- **AES-256-GCM encryption** — optional password-based encryption for archives
- **Incremental sync** — only checks repos that were modified (dirty log)
- **Crash resilience** — detects orphaned extractions and recompresses automatically

## Wipe and Install

`gitka wipe` is the fastest way to set up a fresh USB drive:

```bash
gitka wipe --target /dev/sdb1 --source github --username myuser
```

**Safety guarantees:**
- Refuses to wipe non-removable drives (USB/external only)
- Shows full drive info before any destructive action
- Requires typing `YES` to confirm (not just Enter)
- Auto-selects filesystem: vfat for <4GB, ext4 for larger drives
- Use `--filesystem` to override (ext4, vfat, ntfs)

## Session Tracking (Dirty Log)

Gitka tracks which repos were extracted and what changed during each session. This powers two features:

**Incremental sync** — `gitka sync` skips repos that haven't been touched since last sync, making it much faster with many repos.

**Crash recovery** — if a session wasn't closed cleanly (USB yanked, crash, power loss), `gitka sync` detects the orphaned extraction and recompresses it automatically.

```bash
$ gitka status

Repository Status:
Name                           State           Last Synced     Archive Size        Session
-----------------------------------------------------------------------------------------------
my-project                     Archived        2h ago          45.2 MB
other-repo                     ExtractedLocal  never           12.1 MB             unlocked 30m (2 commits, 5 files)

⚠ 1 repo(s) have active sessions:
  other-repo — unlocked (30m, 2 commits, 5 files touched)

  Run `gitka lock <repo>` to recompress and close session.
```

When you close a session with `gitka lock`, it shows an audit trail of what changed:

```bash
$ gitka lock other-repo

Locking other-repo...
✓ Repo locked and recompressed
  Archive: /mnt/usb/repos/archive/other-repo.gitka.zst (12.3 MB)

  📝 Session audit trail (2 new commit(s)):
    • a1b2c3d4 Fix login bug
    • e5f6g7h8 Add unit tests
```

## Repo States

| State | Description |
|---|---|
| **Archived** | Compressed, at rest — this is the default |
| **Extracted (local)** | Temporarily decompressed for offline commits |
| **Extracted (served)** | Decompressed and served over LAN via GitFlare |

## Configuration

Gitka uses a TOML config file at `<target>/.gitka/gitka.toml`:

```toml
[source]
github_username = "your-username"
# auth_token = "ghp_xxx"  # optional, for private repos

[target]
path = "/mnt/usb"
mode = "removable"

[compression]
backend = "zstd"
tier = "auto"    # auto, low, medium, high

[extraction]
target = "usb"   # or "host" to extract to host computer

[toggles]
clear_after_lock = true
verify_after_sync = true
encryption = false
recovery_records = false
```

View or edit config from the command line:

```bash
gitka config                                   # show full config
gitka config --get source.github_username      # get a value
gitka config --set source.github_username=me   # set a value
```

## Platform Support

| Feature | Linux | macOS | Windows |
|---|---|---|---|
| USB detection | lsblk + /sys/block | diskutil | PowerShell WMI |
| Drive formatting | mkfs | diskutil | diskpart |
| Compression | zstd | zstd | zstd |
| GitFlare serving | ✅ | ✅ | ✅ |

## CLI Commands

| Command | Description |
|---|---|
| `gitka wipe` | Wipe removable drive + set up (SAFETY: removable only, requires YES) |
| `gitka init` | Initialize a new Gitka backup on existing storage |
| `gitka scan` | Discover repos from source, show budget |
| `gitka sync` | Clone/fetch repos (incremental — skips untouched repos) |
| `gitka status` | Per-repo state, archive sizes, active sessions |
| `gitka unlock <repo>` | Extract for offline commit access |
| `gitka lock <repo>` | Recompress + close session (shows audit trail) |
| `gitka serve <repo>` | Serve over LAN via GitFlare |
| `gitka verify` | Check archive integrity |
| `gitka repair <repo>` | Fix corrupted repo with recovery records |
| `gitka config` | View/edit configuration |

## Made By

**Developer/Maintainer: [Sabeeir Sharrma](https://github.com/SabeeirSharrma)**
