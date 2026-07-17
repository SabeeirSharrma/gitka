# Gitka

A Ventoy-inspired tool that creates compressed, physical-media (USB/CD) local copies of all your GitHub/GitFlare repos — with offline commit capability and LAN-based sharing.

## Install

```bash
curl -sSf https://thecinderproject.qd.je/gitka/install.sh | bash
```

This builds Gitka from source and installs it to `/usr/local/bin`.

## Usage

```bash
gitka init --target /mnt/usb --username <github-username>
gitka scan
gitka sync
gitka status
gitka unlock <repo>     # extract for offline work
gitka lock <repo>       # recompress back to archive
gitka serve <repo>      # serve over LAN via GitFlare
gitka verify            # check archive integrity
gitka config            # view/edit config
```

## How It Works

Gitka keeps all your repos compressed on removable media. When you need to work on one, it extracts temporarily, lets you commit offline, then recompresses when you're done.

- **Aggressive compression** — auto-selects zstd tier based on available space
- **Offline commits** — extract a repo, commit locally, sync later
- **LAN sharing** — serve a repo to others on your network via GitFlare
- **Recovery records** — optional par2 redundancy for corruption protection

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
tier = "auto"

[extraction]
target = "usb"  # or "host" to extract to host computer
```

## CLI Commands

| Command | Description |
|---|---|
| `gitka init` | Initialize a new Gitka backup |
| `gitka scan` | Discover repos from source, show budget |
| `gitka sync` | Clone/fetch repos from GitHub/GitFlare |
| `gitka status` | Show per-repo state and archive sizes |
| `gitka unlock <repo>` | Extract for offline commit access |
| `gitka lock <repo>` | Recompress and clear extraction |
| `gitka serve <repo>` | Serve over LAN via GitFlare |
| `gitka verify` | Check archive integrity |
| `gitka repair <repo>` | Fix corrupted repo with recovery records |
| `gitka config` | View/edit configuration |

## Made By

**Developer/Maintainer: [Sabeeir Sharrma](https://github.com/SabeeirSharrma)**
