# Usage Guide

## Quick Start

```bash
# Wipe a fresh USB drive and set up Gitka (interactive, safe)
gitka wipe --target /dev/sdb1 --username <github-username> --token <pat>

# Or initialize without wiping
gitka init --target /mnt/usb --username <github-username> --token <pat>

# Discover repos and check budget
gitka scan

# Clone/pull all repos
gitka sync

# Check what's going on
gitka status
```

The `--token` flag accepts a GitHub personal access token (with `repo` scope for private repos). It can also be persisted via `gitka auth --token <pat>` once and then omitted on subsequent commands.

## Wipe and Install

`gitka wipe` is the fastest way to set up a fresh USB drive:

```bash
gitka wipe --target /dev/sdb1 --source github --username myuser --token <pat>
```

**Safety guarantees:**
- Refuses to wipe non-removable drives (USB/external only)
- Shows full drive info before any destructive action
- Requires typing `YES` to confirm (not just Enter)
- Auto-selects filesystem: vfat for under 4GB, ext4 for larger drives
- Use `--filesystem` to override (ext4, vfat, ntfs)

**Bundles itself on the drive:** After formatting, `gitka wipe` copies the `gitka` CLI and the `gitka-gui` (`gitka-gui.exe` on Windows) binary into `<target>/tools/`. You can run Gitka directly from the USB on any machine — no installed copy required.

## Working Offline

1. **Extract to work offline**
   ```bash
   gitka unlock <repo>
   ```
   This decompresses the repository so you can make local commits. A budget check runs first and warns if the decompressed size might not fit on the target (USB or host, depending on `extraction.target` config).

2. **Commit your changes** using standard Git workflows in the extracted directory.

3. **Lock and recompress**
   ```bash
   gitka lock <repo>
   ```
   This closes the session, audits the work done during the session, and re-archives the repo.

## Sync Algorithm

`gitka sync` runs an incremental fetch/compare/push/pull loop per repo:
- **Ahead only** — local commits not on origin → `push`
- **Behind only** — origin has commits you don't → `pull` (fast-forward)
- **Diverged (no conflicts)** — both sides have independent commits → automatic `merge commit`
- **Diverged (conflicts)** — files modified on both sides → flag for manual resolution, never silently overwrites
- **Equal** — no-op

The algorithm detects each repo's default branch dynamically (`origin/HEAD`, falls back to `main`/`master`/`trunk`) instead of hardcoding `main`, so repos on any default branch work.

## Session Tracking & Incremental Sync

Gitka tracks which repos were extracted and what changed during each session. This powers:
- **Incremental sync**: `gitka sync` skips repos that haven't been touched since last sync. Partial syncs via `--repos a,b` only clear dirty-log entries for the repos they actually synced, so skipped repos don't lose state.
- **Crash recovery**: If a session wasn't closed cleanly (USB yanked, crash, power loss), `gitka sync` detects the orphaned extraction and recompresses it automatically before starting the sync.

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

## GUI

`gitka-gui` (or `gitka-gui.exe` on Windows) is a Tauri-based desktop app that mirrors every CLI command: dashboard, repositories, setup wizard, import, tools (dictionary training + wipe), and settings.

```bash
cd src-tauri && cargo tauri dev    # run the GUI in dev mode
cd src-tauri && cargo tauri build  # build the GUI bundle
```

The GUI talks to the `gitka` CLI over its `--json` outputs (`gitka status --json`, `gitka usb --json`), is fully offline (no CDN), and ships alongside `gitka` so you can launch it from any USB you set up with `gitka wipe`.

## Machine-Readable Output

`gitka status --json` and `gitka usb --json` emit stable JSON shapes for the GUI and any automation that wraps Gitka. Both return an array of objects.

```jsonc
// gitka status --json
[
  {
    "name": "my-repo",
    "state": "Archived",        // "Archived" | "ExtractedLocal" | "ExtractedServed"
    "last_synced": "2h ago",
    "archive_size": "45.2 MB",
    "session": ""               // non-empty when the repo is unlocked/serving
  }
]

// gitka usb --json
[
  { "path": "/run/media/usb", "label": "GITKA", "size": "64003471360B", "mountpoint": "/run/media/usb" }
]
```
