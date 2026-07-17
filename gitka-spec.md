# Gitka — Project Spec (Final, v1.0 draft)

**CLI binary:** `gitka`
**Status:** Not part of The Cinder Project — standalone portfolio project
**One-line pitch:** A Ventoy-inspired tool that creates a compressed, physical-media (USB/CD) local copy of all your GitHub/GitFlare repos — with offline commit capability and LAN-based sharing.

---

## 1. Core Concept

Gitka solves "I want a real, physical, non-cloud copy of all my git repos" — not just a tarball, but a system that:

- Fits many gigabytes of repos onto small, cheap removable media via aggressive but safe compression
- Lets you keep working offline (temporarily-extracted, commit-capable repos) and sync back later
- Lets you temporarily serve a repo to someone else on the same LAN (via a bundled GitFlare instance) without needing cloud access
- Warns rather than silently fails when your choices don't fit your target media or available headroom

---

## 2. Repo States

Every repo lives in exactly one of these states at any time:

| State | Description |
|---|---|
| **Archived** (default, at rest) | Compressed, in `repos/archive/`, browse/extract-only. This is where **every** repo lives when not actively in use — including ones tagged workspace-eligible. |
| **Extracted (local)** | Temporarily decompressed for offline solo commit access. No LAN serving. |
| **Extracted (GitFlare-served)** | Temporarily decompressed and served over LAN via bundled GitFlare, one repo at a time. |

Only repos tagged **workspace-eligible** during setup can ever be extracted. Archive-only repos never leave compressed form.

---

## 3. USB Directory Layout

```
/ (root of USB)
├── gitka(.exe)                    # the one binary — everything runs through this
├── gitka.toml                     # config file
│
├── repos/
│   └── archive/                    # ALL repos live here, compressed, at rest
│       ├── hyprlane-bot.saiarc         # .zst for early versions, .saiarc once SAI backend ships
│       ├── coalbox.saiarc
│       └── wordon-core.saiarc
│
├── extract/                         # TRANSIENT staging area — empty at rest
│   └── (populated only during an active unlock/serve session,
│         unless extraction target is set to "host computer" — see §5)
│
├── tools/
│   ├── gitflare/                      # bundled GitFlare instance (single-repo-serving build)
│   └── recovery/                       # recovery tool binary
│
├── recovery-data/                       # per-repo recovery records — decompressed,
│   ├── hyprlane-bot.par                    stands alone even if archive is corrupted
│   └── coalbox.par
│
└── .gitka/                                # internal state, hidden
    ├── sync-state.toml                        # per-repo HEAD tracking
    └── logs/
```

---

## 4. Target Detection

- **Removable mode** (default): filters to USB/CD only, Rufus/Ventoy-style detection (`removable` flag via WMI on Windows, `/sys/block/*/removable` or `ID_BUS=usb` via udev on Linux). Non-removable drives hidden.
- **Local Backup mode** (explicit opt-in): unlocks non-removable drives. No compression, no archive/extract split — every repo is a plain `git clone`, permanently live. Simplest path, meant for "porting to a new machine" or "this is my working drive."

---

## 5. Extraction & Recompression Lifecycle

Triggered by `gitka unlock <repo>` (local-only) or `gitka serve <repo>` (GitFlare LAN):

```
gitka unlock coalbox          OR       gitka serve coalbox
  → repos/archive/coalbox.saiarc decompressed
  → target: extract/ (on USB) or host computer temp dir — see below
  → [serve only] GitFlare starts, serves over LAN
  → user commits (and/or collaborator pushes via GitFlare)

gitka lock coalbox             OR       gitka serve --stop
  → [serve only] GitFlare server torn down
  → repo recompressed → new repos/archive/coalbox.saiarc
  → verify pass runs BEFORE old archive blob is deleted
  → extraction location cleared (ties into "clear local clone cache" toggle)
```

### Extraction target — opt-in, configurable

By default, extraction happens into `extract/` **on the USB itself**. As an opt-in alternative, Gitka can be configured to extract into a **temp directory on the host computer** the USB is currently plugged into instead. This bypasses USB space pressure entirely for the duration of the unlock/serve session — the recompressed result is written back to `repos/archive/` on the USB once done, same as normal.

- `[extraction] target = "usb" | "host"` in `gitka.toml`
- Host-extraction temp files are cleaned up on `lock`/`serve --stop`, same as USB-side cleanup
- Still opt-in and explicit — using the host disk without asking would be a bad default for a tool whose whole point is "everything lives on the media, not the host"

### Budget check — extended for extraction

Because a repo must temporarily exist in **both compressed and decompressed form** during extraction/recompression, tight-fit drives can fail mid-operation even if the archive itself fits fine at rest.

- At GitFlare/unlock opt-in time (wizard), and again at actual `unlock`/`serve` time (free space may have changed since setup): compute the largest workspace-eligible repo's decompressed size, compare against free space *after* the full archive.
- If tight: **warn explicitly** — e.g. "This repo needs ~X GB temporarily to extract; you have ~Y GB free. Extraction may fail to complete. Consider extracting to host computer instead, or freeing up space." Never silently fails mid-extraction.
- Same shared budget-check logic as the compression-fit check — one function, two trigger points, avoids drift.
- This check is skipped entirely when extraction target is set to "host" (since USB headroom isn't the constraint in that mode) — host-side free space is checked instead.

---

## 6. Compression

**Backend:** zstd initially (proven, widely readable outside Gitka itself — important for a backup tool). Archive header reserves a **pluggable compression-method byte** for future backends (see §11 — SAI).

**Recommended tier, auto-selected from free space vs. scanned repo size:**

| Free space vs. needed | Tier |
|---|---|
| ≥3x | Low/fast (zstd -3 to -9, dedup optional) |
| 1x–3x | Medium (zstd -15 to -19, dedup on) |
| <1x (tight fit) | High (zstd `--ultra -22`, dedup + shallow history default, dictionary on) |
| Won't fit even at max | Flag + propose trimming (shallow specific repos, drop workspace-eligibility for large ones) |

**Settings (own lightweight panel, not WinRAR-scale):**
- Dictionary size: default 32MB, adjustable
- Volume splitting: off by default, size-based when enabled
- Solid archiving: `none` / `per_repo` (default) / `full_archive`
- Cross-repo dedup: content-addressed blob store, on by default

**History depth policy:** per-repo, default shallow (latest commit only), opt-in full history.

---

## 7. Toggles

| Toggle | Default | Notes |
|---|---|---|
| Clear extraction location after lock/serve-stop | On | USB-side or host-side, per extraction target |
| Verify archive integrity + permissions after sync/recompress | On | Read-only source repos stay read-only through the round-trip |
| Encryption (AES-256-GCM) | Off | Per-archive, reuses Coalbox's key-handling approach |
| Recovery records | **Off (opt-in)** | Space-costly; see §8 |

---

## 8. Integrations (opt-in, own wizard page)

### GitFlare LAN Serve
- Serves **one repo at a time**, only workspace-eligible repos, only while extracted
- `git-http-backend`, same as GitFlare's normal operation, pointed at the extraction path (USB or host, per config) instead of a VPS path
- Graceful handling required for USB-yank / crash mid-push — no corrupted archive on recompress

### Recovery Records
- par2-style redundancy, **stored decompressed**, standalone in `recovery-data/`
- Granularity: **per-repo**, regenerated only when that repo's archive blob actually changes
- Opt-in: on tight drives, recovery overhead may be the difference between fitting and not
- Budget check accounts for recovery overhead when flagging over-budget scenarios

---

## 9. Sync Algorithm

Runs per repo, every `gitka sync` (against whichever repos are currently extracted, or against archived repos by extracting → comparing → recompressing transiently):

```
git fetch origin
compare local HEAD vs origin/HEAD
  → local ahead only:      push
  → origin ahead only:     pull (fast-forward)
  → diverged:               attempt auto-merge
                             → clean: proceed
                             → conflict: flag for user, no silent resolution
  → equal:                  no-op
```

Same path handles offline commits, GitFlare-LAN-served commits from a collaborator, and normal multi-machine drift. Recovery record regenerates only if the repo's archive blob changed.

---

## 10. CLI Surface

```
gitka                    # no subcommand → launches GUI
gitka init                 # wizard: source auth, selection, target, compression, integrations
gitka scan                   # re-scan sources + target, show size report + budget check
gitka sync                     # fetch/compare/push/pull/merge loop across selected repos
gitka status                     # per-repo state: ahead/behind/conflict, last synced
gitka unlock <repo>                # extract for local-only offline commit access
gitka lock <repo>                    # recompress + clear extraction, end local-only session
gitka serve <repo>                     # extract + start GitFlare LAN server
gitka serve --stop                       # stop LAN server, recompress, clear extraction
gitka verify                               # manual integrity + permissions check
gitka repair <repo>                          # use recovery record to fix a corrupted repo
gitka config                                   # view/edit TOML config
gitka gui                                        # explicitly launch GUI (same as bare `gitka`)
```

---

## 11. Compression Backend Roadmap — SAI

Gitka ships on zstd from day one. Archive header's pluggable compression-method byte allows swapping backends without breaking old archives.

**SAI** (separate project, own repo/versioning/timeline — see `sai-spec.md`) is a general-purpose compression engine being built independently, aimed at beating category-leading compressors on structured/text-heavy data through content-aware dedup + targeted tuning. Not Gitka-exclusive. Once SAI reaches a stable v1.0, it becomes an additive backend option in Gitka — never a required rewrite. Gitka's cross-repo dedup is effectively a specific case of SAI's general chunk-level dedup, so long-term this removes duplicate engineering between the two projects.

---

## 12. Config

TOML, matching the Cinder Project convention (Gitka itself is not a Cinder project, but reuses the format for consistency across Sabeeir's tools). Full schema in companion file `config-schema.toml` — covers source(s), target, selection with per-repo workspace-eligibility, compression settings, extraction target, toggles, integrations, budget state, and per-repo sync state.

---

## 13. Open / Deferred Items

- GUI wireframes (Tauri or egui — leaning Tauri for cross-platform consistency and shared patterns with SAI's own GUI, TBD)
- Future integrations beyond GitFlare + Recovery (e.g. generic git-remote adapter, read-only web viewer)
- CD edition specifics: burn-once snapshot mode, diff-patch export for later USB merge
- GitFlare bundle: full install vs. stripped single-repo-serving build (space tradeoff on tight media)

---

## 14. Naming

- **Project:** Gitka
- **CLI binary:** `gitka`
- Considered and rejected for collision: Cinder-pattern names (out of scope), Satchel-family names, `ugit` (heavily taken), `crepo`/`urepo` (real existing tools in adjacent space)
