# CLI Command Reference

Below is a complete list of Gitka commands and their descriptions.

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
| `gitka serve <repo> --stop` | Stop the LAN server |
| `gitka verify` | Check archive integrity |
| `gitka repair <repo>` | Fix corrupted repo with recovery records |
| `gitka config` | View/edit configuration |
