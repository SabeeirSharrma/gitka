# Installation

Gitka is easy to install via our cross-platform install scripts. They build from source and place the binary on your PATH.

## Requirements
- **All platforms:** A working network connection for the initial build.
- **Linux/macOS:** Rust toolchain (`cargo`) — installed automatically if missing.
- **Windows:** Git for Windows (installed automatically if missing), Rust toolchain (installed automatically), MSVC C++ build tools (installed automatically).

## Linux or macOS

```bash
curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash
```

The installer detects your OS, installs Rust via rustup if needed, builds Gitka release binaries, and copies them to `/usr/local/bin` (requires `sudo` if not writable).

To install to a different location:

```bash
curl -sSf https://sabeeir.qd.je/gitka/install.sh | bash -s -- --prefix ~/.local
# then: export PATH="$HOME/.local/bin:$PATH"
```

## Windows

```powershell
[Net.ServicePointManager]::SecurityProtocol = 'tls12'
irm https://sabeeir.qd.je/gitka/install-windows.ps1 | iex
```

The PowerShell installer downloads and installs any missing prerequisites (Git, Rust, MSVC Build Tools), builds Gitka, and copies `gitka.exe` to `%LOCALAPPDATA%\Programs\Gitka\` (added to your user PATH).

If `gitka` is not found in PowerShell afterward, open a new terminal so PATH changes apply.

## Verify

```bash
gitka --version
```

Should print `gitka 0.6.0` after the v0.6.0 release.

## Platform support

| Feature | Linux | macOS | Windows |
|---|---|---|---|
| USB detection | lsblk + /sys/block | diskutil (plist) | PowerShell WMI |
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
| Private repo sync | ✅ | ✅ | ✅ |
| GUI (gitka-gui) | ✅ | ✅ | ✅ |
