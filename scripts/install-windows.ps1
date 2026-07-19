# Gitka installer for Windows (PowerShell)
# Builds CLI and GUI from source and installs both to a location in PATH.
#
# Usage (run as Administrator, or ensure your user profile is in PATH):
#   [Net.ServicePointManager]::SecurityProtocol = 'tls12'
#   iex "& { $(irm https://sabeeir.qd.je/gitka/install-windows.ps1) }"
#
# Or download and run locally:
#   powershell -ExecutionPolicy Bypass -File install-windows.ps1

param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\Gitka",
    [switch]$SkipRustInstall,
    [switch]$NoConfirm,
    [switch]$CliOnly
)

$ErrorActionPreference = "Stop"
$InformationPreference = "Continue"

# ── Helper functions ──────────────────────────────────────────────────
function Write-Info  { Write-Host "▸ $args" -ForegroundColor Blue }
function Write-Ok    { Write-Host "✓ $args" -ForegroundColor Green }
function Write-Warn  { Write-Host "⚠ $args" -ForegroundColor Yellow }
function Write-Err   { Write-Host "✗ $args" -ForegroundColor Red; exit 1 }

# ── Preflight: are we in a valid shell? ───────────────────────────────
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)

Write-Info "Gitka installer for Windows"
Write-Info "Install directory: $InstallDir"
Write-Info "Administrator: $isAdmin"
Write-Info ""

# ── Check git ─────────────────────────────────────────────────────────
$gitPath = Get-Command "git" -ErrorAction SilentlyContinue
if (-not $gitPath) {
    Write-Warn "Git is not installed. Gitka requires git for cloning repositories."

    $installGit = $true
    if (-not $NoConfirm) {
        $response = Read-Host "Install Git for Windows now? (Y/n)"
        if ($response -ne "" -and $response -ne "y" -and $response -ne "Y") {
            $installGit = $false
        }
    }

    if ($installGit) {
        Write-Info "Downloading Git for Windows..."
        $gitUrl = "https://github.com/git-for-windows/git/releases/download/v2.47.1.windows.1/Git-2.47.1-64-bit.exe"
        $gitInstaller = "$env:TEMP\git-installer.exe"
        [Net.ServicePointManager]::SecurityProtocol = 'tls12'
        Invoke-WebRequest -Uri $gitUrl -OutFile $gitInstaller -UseBasicParsing

        Write-Info "Installing Git (this will open the installer)..."
        Start-Process -Wait -FilePath $gitInstaller -ArgumentList "/VERYSILENT", "/NORESTART", "/NOCANCEL", "/SP-", "/SUPPRESSMSGBOXES", "/DIR=C:\Program Files\Git"
        Remove-Item $gitInstaller -ErrorAction SilentlyContinue

        # Add to PATH for this session
        $env:Path = "C:\Program Files\Git\bin;$env:Path"
        Write-Ok "Git installed."
    } else {
        Write-Err "Git is required. Install it manually: https://git-scm.com"
    }
} else {
    Write-Ok "Git found at $($gitPath.Source)"
}

# ── Check / install Rust ──────────────────────────────────────────────
$cargoPath = Get-Command "cargo" -ErrorAction SilentlyContinue
if (-not $cargoPath) {
    if (-not $SkipRustInstall) {
        Write-Info "Rust/Cargo not found. Installing via rustup..."
        $rustupUrl = "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe"
        $rustupInstaller = "$env:TEMP\rustup-init.exe"
        [Net.ServicePointManager]::SecurityProtocol = 'tls12'
        Invoke-WebRequest -Uri $rustupUrl -OutFile $rustupInstaller -UseBasicParsing

        Start-Process -Wait -FilePath $rustupInstaller -ArgumentList "-y", "--default-host", "x86_64-pc-windows-msvc"
        Remove-Item $rustupInstaller -ErrorAction SilentlyContinue

        # Source cargo into this session
        $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { "$env:USERPROFILE\.cargo" }
        $env:Path = "$cargoHome\bin;$env:Path"
        Write-Ok "Rust installed."
    } else {
        Write-Err "Rust/Cargo is required. Install it first: https://rustup.rs"
    }
}

$cargoPath = Get-Command "cargo" -ErrorAction SilentlyContinue
if (-not $cargoPath) {
    Write-Err "Cargo still not available after install."
}
Write-Ok "Cargo found at $($cargoPath.Source)"

# ── Ensure MSVC build tools ───────────────────────────────────────────
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$msvcFound = $false
if (Test-Path $vswhere) {
    $vsPath = & $vswhere -latest -property installationPath 2>$null
    if ($vsPath) {
        $msvcFound = $true
    }
}

if (-not $msvcFound) {
    Write-Warn "Visual Studio C++ build tools not detected."
    Write-Warn "Gitka requires the MSVC toolchain (cl.exe, link.exe, lib.exe)."

    $installMsvc = $true
    if (-not $NoConfirm) {
        $response = Read-Host "Install Visual Studio 2022 Build Tools with C++ workload now? (Y/n)"
        if ($response -ne "" -and $response -ne "y" -and $response -ne "Y") {
            $installMsvc = $false
        }
    }

    if ($installMsvc) {
        Write-Info "Downloading Visual Studio 2022 Build Tools installer..."
        $vsUrl = "https://aka.ms/vs/17/release/vs_BuildTools.exe"
        $vsInstaller = "$env:TEMP\vs_BuildTools.exe"
        [Net.ServicePointManager]::SecurityProtocol = 'tls12'
        Invoke-WebRequest -Uri $vsUrl -OutFile $vsInstaller -UseBasicParsing

        Write-Info "Installing MSVC build tools (this takes a while)..."
        $arguments = "--quiet", "--wait", "--norestart", "--nocache"
        $arguments += "--add", "Microsoft.VisualStudio.Workload.VCTools"
        $arguments += "--remove", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64"
        $arguments += "--includeRecommended"

        $proc = Start-Process -Wait -FilePath $vsInstaller -ArgumentList $arguments -PassThru -NoNewWindow
        Remove-Item $vsInstaller -ErrorAction SilentlyContinue

        if ($proc.ExitCode -eq 0 -or $proc.ExitCode -eq 3010) {
            Write-Ok "MSVC build tools installed."
        } else {
            Write-Warn "MSVC install may have failed (exit code: $($proc.ExitCode))."
            Write-Warn "Try installing manually: https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022"
        }
    } else {
        Write-Warn "Proceeding without MSVC tools. Build may fail."
    }
} else {
    Write-Ok "MSVC build tools detected."
}

# ── Clone & build ─────────────────────────────────────────────────────
$buildDir = "$env:TEMP\gitka-build-$([System.IO.Path]::GetRandomFileName())"

try {
    Write-Info "Cloning Gitka..."
    & git clone --depth 1 "https://github.com/SabeeirSharrma/gitka.git" $buildDir 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "git clone failed" }
    Write-Ok "Repository cloned."

    # ── Build CLI ─────────────────────────────────────────────────
    Write-Info "Building CLI (this may take several minutes)..."
    Push-Location $buildDir
    try {
        $buildOutput = & cargo build --release --bin gitka 2>&1 | Out-String
        if ($LASTEXITCODE -ne 0) {
            Write-Warn "Build output:"
            Write-Host $buildOutput -ForegroundColor DarkYellow
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
    Write-Ok "CLI build completed."

    $cliBinaryPath = "$buildDir\target\release\gitka.exe"
    if (-not (Test-Path $cliBinaryPath)) {
        throw "CLI binary not found at $cliBinaryPath"
    }

    # ── Install CLI ───────────────────────────────────────────────
    Write-Info "Installing CLI to $InstallDir..."
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item $cliBinaryPath "$InstallDir\gitka.exe" -Force
    Write-Ok "Installed to $InstallDir\gitka.exe"

    # ── Build & install GUI (unless --cli-only) ───────────────────
    if (-not $CliOnly -and (Test-Path "$buildDir\src-tauri")) {
        Write-Info "Building GUI..."

        # Ensure tauri-cli is available
        $tauriAvailable = $true
        $tauriCheck = & cargo tauri --version 2>&1
        if ($LASTEXITCODE -ne 0) {
            Write-Info "Installing tauri-cli..."
            & cargo install tauri-cli --locked 2>&1 | Out-Null
            if ($LASTEXITCODE -ne 0) {
                Write-Warn "Could not install tauri-cli. GUI not built."
                $tauriAvailable = $false
            }
        }

        if ($tauriAvailable) {
            Push-Location "$buildDir\src-tauri"
            try {
                # cargo tauri build may fail at bundling but the binary is built
                $guiBuildOutput = & cargo tauri build 2>&1 | Out-String
                if ($LASTEXITCODE -eq 0) {
                    # Find the built binary
                    $guiBinaryPath = "$buildDir\src-tauri\target\release\gitka-gui.exe"
                    if (Test-Path $guiBinaryPath) {
                        Copy-Item $guiBinaryPath "$InstallDir\gitka-gui.exe" -Force
                        Write-Ok "Installed to $InstallDir\gitka-gui.exe"
                    } else {
                        Write-Warn "GUI binary not found at expected location."
                    }
                } else {
                    Write-Warn "GUI build failed. CLI was installed successfully."
                }
            } finally {
                Pop-Location
            }
        }
    } elseif ($CliOnly) {
        Write-Info "Skipping GUI (--cli-only flag)"
    }

    # ── Add to PATH if not already ─────────────────────────────────
    $userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
    if ($userPath -notlike "*$InstallDir*") {
        $newPath = if ($userPath) { "$InstallDir;$userPath" } else { $InstallDir }
        [Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
        $env:Path = "$InstallDir;$env:Path"
        Write-Ok "Added $InstallDir to user PATH"
    }

    # ── Verify ─────────────────────────────────────────────────────
    $installed = Get-Command "gitka" -ErrorAction SilentlyContinue
    if ($installed) {
        $version = & gitka --version 2>&1 | Out-String
        Write-Ok "Gitka is ready!"
        Write-Host "  $version" -ForegroundColor Gray
    } else {
        Write-Warn "Installed but 'gitka' not found in PATH."
        Write-Warn "Make sure $InstallDir is in your PATH or log out and back in."
    }

} finally {
    if (Test-Path $buildDir) {
        Remove-Item -Recurse -Force $buildDir -ErrorAction SilentlyContinue
    }
}

Write-Host ""
Write-Host "  Quick start:" -ForegroundColor Cyan
Write-Host "    gitka init --target D:\ --username <github-user> --token <pat>" -ForegroundColor Gray
Write-Host "    gitka scan && gitka sync" -ForegroundColor Gray
Write-Host "    gitka status" -ForegroundColor Gray
Write-Host "    gitka update" -ForegroundColor Gray
Write-Host ""
