# Gitka

A Ventoy-inspired tool that creates compressed, physical-media (USB/CD) local copies of all your GitHub/GitFlare repos — with offline commit capability and LAN-based sharing.

## Install

```bash
curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash
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
gitka import <path>                      # Import a local git repo into backup
gitka train-dict                         # Train zstd dictionary for better compression
```

## How It Works

Gitka keeps all your repos compressed on removable media. When you need to work on one, it extracts temporarily, lets you commit offline, then recompresses when you're done.

- **Aggressive compression** — auto-selects zstd tier based on available space
- **Zstd dictionary training** — trains a shared dictionary from repo content for better small-file compression
- **Archive format header** — every archive starts with a 12-byte GITKA header (magic + version + compression method) for future-proofing
- **Offline commits** — extract a repo, commit locally, sync later
- **LAN sharing** — serve a repo to others on your network via GitFlare
- **Recovery records** — optional per-part par2 redundancy for corruption protection
- **AES-256-GCM encryption** — optional per-volume password-based encryption for archives
- **Incremental sync** — only checks repos that were modified (dirty log)
- **Crash resilience** — detects orphaned extractions and recompresses automatically
- **Volume splitting** — split archives into fixed-size parts for CD/DVD or FAT32 limits
- **Cross-repo dedup** — share common blobs across repos to save space
- **FullArchive mode** — compress all repos into a single solid archive stream
- **Local repo import** — import existing git clones directly into backup

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

## Archive Format

Every archive starts with a 12-byte header that identifies it as a Gitka archive:

| Offset | Size | Field | Description |
|---|---|---|---|
| 0 | 5 | Magic | `GITKA` |
| 5 | 1 | Version | Format version (currently 1) |
| 6 | 1 | Compression | Method byte (`0x01` = zstd) |
| 7 | 5 | Reserved | Reserved for future use |

This header is written only to the first volume part. Multi-volume archives use the standard split naming: `repo.gitka.zst` (part 1), `repo.gitka.zst.002` (part 2), etc.

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

**Per-volume encryption:** When encryption is enabled, each volume part is encrypted independently with its own AES-256-GCM nonce. This means each part can be decrypted without loading the entire archive.

**Per-volume recovery:** When recovery records are enabled, par2 redundancy is generated per-part, so individual corrupted parts can be repaired independently.

## Zstd Dictionary Training

Gitka can train a zstd dictionary from your repo content to improve compression of small files. The dictionary captures common patterns across your files (e.g., shared libraries, config templates, vendored dependencies).

```bash
# Train a dictionary from archived repos
gitka train-dict

# Train from a specific directory
gitka train-dict --source /path/to/samples
```

The trained dictionary is saved to `.trained.dict` alongside the archive and is automatically used for all future compress/decompress operations. Dictionary size is controlled by `compression.dictionary_size_mb` (default: 32 MB).

## FullArchive Mode

By default, each repo gets its own archive (PerRepo mode). FullArchive mode compresses all repos into a single solid archive stream, which can improve compression ratio when repos share common content.

```bash
gitka config --set compression.solid=full_archive
```

When syncing in FullArchive mode, all repos are collected into a staging directory and compressed into one `full-archive.gitka.zst` file. Each repo's metadata points to this shared archive.

Available modes:
- `none` — per-file compression (individual zstd frames)
- `per_repo` — each repo gets its own archive (default)
- `full_archive` — all repos in a single archive stream

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

## Import Local Repos

Import an existing local git repository into your Gitka backup without needing to clone from a remote source.

```bash
# Import by path (uses directory name as repo name)
gitka import /path/to/my-repo

# Import with a custom name
gitka import /path/to/my-repo --name my-project
```

The import command:
1. Verifies the path is a git repository
2. Compresses the repo into a `.gitka.zst` archive
3. Creates metadata entries
4. Adds the repo to your config
5. Optionally encrypts and creates recovery records

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
dictionary_size_mb = 32          # zstd dictionary size for better small-file compression
solid = "per_repo"               # none, per_repo (default), full_archive
dedup = true                     # cross-repo deduplication
# volume_splitting = { size_mb = 4096 }  # uncomment to enable splitting

[extraction]
target = "usb"   # or "host" to extract to host computer

[toggles]
clear_after_lock = true
verify_after_sync = true
encryption = false               # AES-256-GCM per-volume encryption
recovery_records = false         # par2 per-volume recovery records
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
| Archive header | ✅ | ✅ | ✅ |
| Zstd dictionary | ✅ | ✅ | ✅ |
| Volume splitting | ✅ | ✅ | ✅ |
| Per-volume encryption | ✅ | ✅ | ✅ |
| Per-volume recovery | ✅ | ✅ | ✅ |
| Cross-repo dedup | ✅ | ✅ | ✅ |
| FullArchive mode | ✅ | ✅ | ✅ |
| Local repo import | ✅ | ✅ | ✅ |
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
| `gitka import <path>` | Import a local git repo into backup |
| `gitka train-dict` | Train zstd dictionary for better compression |

## Made By

**Developer/Maintainer: [Sabeeir Sharrma](https://github.com/SabeeirSharrma)**
