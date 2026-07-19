# Configuration

Gitka uses a TOML config file located at `<target>/.gitka/gitka.toml`. A companion schema with all fields and defaults is at [`config-schema.toml`](../config-schema.toml) in the repo root.

## Example Config

```toml
[source]
github_username = "your-username"
# auth_token = "ghp_xxx"  # optional, for private repos (run `gitka auth --token <pat>` to set)

[target]
path = "/mnt/usb"
mode = "removable"  # or "local" to opt-in to non-removable drives

[repos]  # auto-managed by `gitka sync` / `gitka import`
# name = "my-project"
# workspace_eligible = true    # can be unlocked for offline work
# full_history = false         # shallow clone (latest commit only)
# last_synced = "abc123..."

[compression]
backend = "zstd"
tier = "auto"                    # auto, low, medium, high
dictionary_size_mb = 32          # zstd dictionary size for small-file compression
solid = "per_repo"               # none, per_repo (default), full_archive
dedup = true                     # cross-repo deduplication
# volume_splitting = { size_mb = 4096 }  # uncomment to enable splitting

[extraction]
target = "usb"   # or "host" to extract to host computer's temp dir

[toggles]
clear_after_lock = true
verify_after_sync = true
encryption = false               # AES-256-GCM per-volume encryption (auto-generates persisted salt)
recovery_records = false         # par2 per-volume recovery records (~25% overhead)

[encryption]  # auto-populated when toggles.encryption is enabled
# password = "..."               # optional, can use GITKA_PASSWORD env var
# salt = "abcdef0123456789..."   # 32-char hex, persisted for cross-session recovery

[integrations.gitflare]
port = 8080                      # LAN server port for `gitka serve`
bind_address = "0.0.0.0"
```

## CLI Configuration Management

View or edit config directly from the command line:

```bash
gitka config                                       # show full config
gitka config --get source.github_username          # get a value
gitka config --set source.github_username=me       # set a value
gitka config --get compression.dedup               # check dedup status
gitka config --get compression.volume_splitting.size_mb  # check split size
gitka config --set toggles.encryption=true         # turn on encryption
gitka config --set toggles.recovery_records=true   # turn on recovery records
gitka config --set extraction.target=host          # extract to host disk
```

## Per-repo workspace eligibility

By default every repo is **workspace-eligible** (can be unlocked/serialized). To mark a specific repo as archive-only (never extracted), use `gitka config --set repos.<name>.workspace_eligible=false`. Archive-only repos stay permanently compressed and are never offered for extraction.

## Encryption salt persistence

When encryption is enabled (during `init`, `wipe`, or `config --set togglers.encryption=true`), Gitka generates a 16-byte random salt and persists it (hex-encoded) to the config. Key derivation uses an iterated SHA-256 chain over (password, salt, round index) for 10,000 rounds. The salt is required to recover encrypted archives across sessions and machines — never edit it manually.

## Where Gitka looks for the config

In order:
1. `--config <path>` argument (CLI, global)
2. `./gitka.toml` (current directory)
3. `./.gitka/gitka.toml` (current directory)
4. `~/.gitka/gitka.toml` (home directory)
