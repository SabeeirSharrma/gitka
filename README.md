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
gitka wipe --target <device>             # Wipe removable drive + set up (REMOVABLE ONLY)
gitka init --target <path>               # Initialize without wiping
gitka init --target <path> --volume-size 4096  # Initialize with FAT32 volume splitting
gitka init --target <path> --dedup       # Initialize with cross-repo dedup
gitka scan                               # Discover repos, show budget
gitka sync                               # Clone/fetch repos from source
gitka status                             # Per-repo state + active sessions
gitka unlock <repo>                      # Extract for offline work
gitka lock <repo>                        # Recompress + close session
gitka serve <repo>                       # Serve over LAN via GitFlare
gitka serve <repo> --stop                # Stop the server
gitka verify                             # Check archive integrity
gitka repair <repo>                      # Fix corrupted repo with recovery records
gitka config                             # View/edit config
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
- **Volume splitting** — split archives into fixed-size parts for CD/DVD or FAT32 limits
- **Cross-repo dedup** — share common blobs across repos to save space

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

## Volume Splitting

Split archives into fixed-size parts. Useful for burning to CD/DVD or working around FAT32's 4 GB file size limit.

```bash
# Enable during init (4 GB parts for FAT32)
gitka init --target /mnt/usb --volume-size 4096

# Or enable via config
gitka config --set compression.volume_splitting.size_mb=700  # CD-sized parts
gitka config --set compression.volume_splitting.size_mb=4096  # FAT32 limit
gitka config --set compression.volume_splitting.size_mb=off   # disable
```

When enabled, archives are split across multiple files:
```
repos/archive/my-project.gitka.zst      # Part 1 (primary)
repos/archive/my-project.gitka.zst.002  # Part 2
repos/archive/my-project.gitka.zst.003  # Part 3 (if needed)
```

All operations (lock, unlock, verify, decompress) automatically detect and handle multi-volume archives. You can copy all parts to your media and Gitka will reassemble them transparently.

## Cross-Repo Deduplication

When multiple repos share common files (e.g., shared libraries, node_modules, vendored dependencies), dedup saves space by storing each unique content block only once.

```bash
# Enable during init
gitka init --target /mnt/usb --dedup

# Or enable/disable via config
gitka config --set compression.dedup=true
gitka config --set compression.dedup=false
```

Dedup works by hashing each file's content with SHA-256 before compression. If the same content already exists in the dedup store (from a previously compressed repo), only a reference is written instead of the full content. The dedup store is shared across all repos in the same backup target.

**Space savings example:** If 3 repos each contain the same 50 MB vendored dependency, dedup stores it once instead of three times, saving ~100 MB.

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
  Dedup saved: 3.1 MB

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
tier = "auto"                    # auto, low, medium, high
dictionary_size_mb = 32
dedup = true                     # cross-repo deduplication
# volume_splitting = { size_mb = 4096 }  # uncomment to enable splitting

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
gitka config --get compression.dedup           # check dedup status
gitka config --get compression.volume_splitting.size_mb  # check split size
```

## Platform Support

| Feature | Linux | macOS | Windows |
|---|---|---|---|
| USB detection | lsblk + /sys/block | diskutil | PowerShell WMI |
| Drive formatting | mkfs | diskutil | diskpart |
| Compression | zstd | zstd | zstd |
| Volume splitting | ✅ | ✅ | ✅ |
| Cross-repo dedup | ✅ | ✅ | ✅ |
| GitFlare serving | ✅ | ✅ | ✅ |

## CLI Commands

| Command | Description |
|---|---|
| `gitka wipe` | Wipe removable drive + set up (SAFETY: removable only, requires YES) |
| `gitka init` | Initialize a new Gitka backup on existing storage |
| `gitka init --volume-size <MB>` | Initialize with volume splitting |
| `gitka init --dedup` | Initialize with cross-repo deduplication |
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
