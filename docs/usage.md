# Usage Guide

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

## Working Offline

1. **Extract to work offline**
   ```bash
   gitka unlock <repo>
   ```
   This decompresses the repository so you can make local commits.

2. **Commit your changes** using standard Git workflows in the extracted directory.

3. **Lock and recompress**
   ```bash
   gitka lock <repo>
   ```
   This closes the session and re-archives the repo.

## Session Tracking & Incremental Sync

Gitka tracks which repos were extracted and what changed during each session. This powers:
- **Incremental sync**: `gitka sync` skips repos that haven't been touched since last sync.
- **Crash recovery**: If a session wasn't closed cleanly, `gitka sync` detects the orphaned extraction and recompresses it automatically.

```bash
$ gitka status

Repository Status:
Name                           State           Last Synced     Archive Size        Session
-----------------------------------------------------------------------------------------------
my-project                     Archived        2h ago          45.2 MB
other-repo                     ExtractedLocal  never           12.1 MB             unlocked 30m (2 commits, 5 files)

⚠ 1 repo(s) have active sessions:
  other-repo — unlocked (30m, 2 commits, 5 files touched)
```

## Repo States

| State | Description |
|---|---|
| **Archived** | Compressed, at rest — this is the default |
| **Extracted (local)** | Temporarily decompressed for offline commits |
| **Extracted (served)** | Decompressed and served over LAN via GitFlare |
