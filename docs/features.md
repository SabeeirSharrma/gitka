# Advanced Features

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

When multiple repos share common files (e.g., shared libraries, `node_modules`, vendored dependencies), dedup saves space by storing each unique content block only once.

```bash
# Enable during init
gitka init --target /mnt/usb --dedup

# Or enable/disable via config
gitka config --set compression.dedup=true
gitka config --set compression.dedup=false
```

Dedup hashes each file's content with SHA-256 before compression. If the same content already exists in the dedup store (from a previously compressed repo), only a reference is written instead of the full content. The dedup store is shared across all repos in the same backup target.

**Space savings example:** If 3 repos each contain the same 50 MB vendored dependency, dedup stores it once instead of three times, saving ~100 MB.

## Encryption

Gitka supports optional **AES-256-GCM per-volume encryption** for archives.

```bash
gitka config --set toggles.encryption=true
gitka auth --token <pat>  # also prompts to set GITKA_PASSWORD (or set it via env)
```

When encryption is enabled:
- Each volume part is encrypted independently with its own nonce.
- Each part can be decrypted without loading the entire archive.
- A 16-byte random salt is generated and persisted to config when turned on (during `init`, `wipe`, or `config --set`), so encrypted archives are recoverable across sessions and machines.
- Key derivation uses an iterated SHA-256 chain over (password, salt, round index) for 10,000 rounds.

You can set the encryption password via:
- `GITKA_PASSWORD` environment variable
- `gitka config --set encryption.password=<value>` (avoid on shared systems — it's in plaintext)

The salt is auto-generated. **Never edit `encryption.salt` manually** unless you know exactly what you're doing — losing the salt means losing access to all encrypted archives.

## Recovery Records

Optional **par2-style redundancy** for corruption protection, stored in `recovery-data/` per-repo.

```bash
gitka config --set toggles.recovery_records=true
```

When enabled:
- Each archive volume part gets its own `.par2` recovery records.
- Individual corrupted parts can be repaired independently with `gitka repair <repo>`.
- Overhead is approximately **25% of the archive size**. The budget check accounts for this so over-budget warnings fire before you run out of space mid-compress.

Recovery is pre-flight checked: `gitka verify` will warn if recovery records are present but invalid.

## FullArchive Mode

By default each repo gets its own archive (`PerRepo`). FullArchive mode compresses all repos into a single solid archive stream, which can improve compression ratio when repos share common content (similar to `.tar.zst` over individual files).

```bash
gitka config --set compression.solid=full_archive
```

When syncing in FullArchive mode, all repos are collected into a staging directory and compressed into one `full-archive.gitka.zst` file. Each repo's metadata points to this shared archive.

Available modes:
- `none` — per-file compression (individual zstd frames)
- `per_repo` — each repo gets its own archive (default)
- `full_archive` — all repos in a single archive stream

## Zstd Dictionary Training

Train a zstd dictionary from your repo content to improve compression of small files. The dictionary captures common patterns across your files (e.g., shared libraries, config templates, vendored dependencies).

```bash
# Train a dictionary from archived repos
gitka train-dict

# Train from a specific directory
gitka train-dict --source /path/to/samples
```

The trained dictionary is saved to `.trained.dict` alongside the archive and is automatically used for all future compress/decompress operations. Dictionary size is controlled by `compression.dictionary_size_mb` (default: 32 MB).

## Auto-Merge on Divergence

`gitka sync` runs an incremental fetch/compare/push/pull loop per repo. When local and remote branches diverge:
- **No conflicts** → automatic merge commit is created (same behavior as `git pull --no-rebase`).
- **Conflict** → flagged for manual resolution, never silently overwrites.

The algorithm dynamically detects each repo's default branch (`origin/HEAD`, falls back to `main` / `master` / `trunk`). Repos on any default branch work with `gitka sync`.

## Host-side Extraction

Gitka can extract a repo to the host computer's temp directory instead of the USB itself, bypassing USB space pressure during long unlock/serve sessions. Recommended for tight-fit drives.

```bash
gitka config --set extraction.target=host
```

When `extraction.target = "host"`:
- The repo decompresses to a temp dir on the host machine.
- Recompressed result is written back to `<target>/.gitka/extract/` if USB or `<target>/repos/archive/` if remote, same as the `usb` default.
- Temp files are cleaned up on `lock` / `serve --stop`.
- Budget checks use host disk free space (not the USB) — accurate "tight fit" warnings.

This is an **opt-in** alternative. The default `usb` is the right behavior for a tool whose purpose is "everything lives on the media, not the host."

## Crash Recovery

If a session wasn't closed cleanly (USB yanked, crash, power loss), `gitka sync` automatically detects the orphaned extraction at startup and recompresses it before continuing. You don't have to manually repair anything.
