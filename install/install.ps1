# slopcop installer for Windows (PowerShell 5.1 or 7).
#
# Downloads one released slopcop binary from GitHub over HTTPS, verifies it
# against the checksum published next to it, installs it to
# %LOCALAPPDATA%\Programs\slopcop (no administrator rights), and adds that
# folder to the user PATH. Nothing else is downloaded or executed.
#
# Environment:
#   SLOPCOP_SOURCE    set to 1 to build the current source from GitHub
#                     instead of downloading a release (needs cargo)
#   SLOPCOP_VERSION   release tag to install, e.g. v0.1.1, or with
#                     SLOPCOP_SOURCE the branch, tag or commit to build
#                     (default: latest release, main from source)
#   SLOPCOP_PREFIX    install directory
#   SLOPCOP_BASE_URL  alternative release location (https:// URL or local
#                     folder), for mirrors and tests; needs SLOPCOP_VERSION
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-StrictMode -Version 3

$Repo = 'JGalego/slopcop'
$Version = if ($env:SLOPCOP_VERSION) { $env:SLOPCOP_VERSION } else { 'latest' }
$Prefix = if ($env:SLOPCOP_PREFIX) { $env:SLOPCOP_PREFIX } else { Join-Path $env:LOCALAPPDATA 'Programs\slopcop' }

function Say($msg) { Write-Host "slopcop-install: $msg" }
# throw instead of exit: under `irm | iex` exit would close the user's shell.
function Die($msg) { throw "slopcop-install: error: $msg" }

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("slopcop-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    New-Item -ItemType Directory -Force -Path $Prefix | Out-Null
    if ($env:SLOPCOP_SOURCE -eq '1') {
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
            Die 'building from source needs cargo: https://rustup.rs'
        }
        $rev = @()
        $label = 'main'
        if ($Version -ne 'latest') { $rev = @('--rev', $Version); $label = $Version }
        Say "building https://github.com/$Repo ($label) with $(cargo --version)"
        & cargo install --locked --root $tmp --git "https://github.com/$Repo" @rev slopcop
        if ($LASTEXITCODE -ne 0) { Die "build failed; check that $label exists in https://github.com/$Repo" }
        Copy-Item -Force (Join-Path $tmp 'bin\slopcop.exe') (Join-Path $Prefix 'slopcop.exe')
    }
    else {
        # Only an x64 build is released; Windows on ARM runs it under emulation.
        if ($env:PROCESSOR_ARCHITECTURE -notin @('AMD64', 'ARM64')) {
            Die "unsupported CPU architecture $($env:PROCESSOR_ARCHITECTURE) (need AMD64 or ARM64)"
        }
        if ($Version -eq 'latest') {
            if ($env:SLOPCOP_BASE_URL) { Die 'SLOPCOP_BASE_URL needs SLOPCOP_VERSION' }
            $Version = (Invoke-RestMethod -UseBasicParsing -Uri "https://api.github.com/repos/$Repo/releases/latest").tag_name
            if (-not $Version) { Die 'could not look up the latest release' }
        }
        $base = if ($env:SLOPCOP_BASE_URL) { $env:SLOPCOP_BASE_URL } else { "https://github.com/$Repo/releases/download/$Version" }
        $local = Test-Path -LiteralPath $base -PathType Container
        if (-not $local -and -not $base.StartsWith('https://')) { Die "refusing non-HTTPS download location: $base" }

        function Fetch($name, $dest) {
            if ($local) { Copy-Item -LiteralPath (Join-Path $base $name) -Destination $dest }
            else { Invoke-WebRequest -UseBasicParsing -Uri "$base/$name" -OutFile $dest }
        }

        $stem = "slopcop-$Version-x86_64-pc-windows-msvc"
        $asset = "$stem.zip"
        Say "downloading $asset from $base"
        Fetch $asset (Join-Path $tmp $asset)
        Fetch "$stem.sha256" (Join-Path $tmp 'sha256')

        $want = ((Get-Content -Raw (Join-Path $tmp 'sha256')) -split '\s+')[0].ToLower()
        $got = (Get-FileHash -Algorithm SHA256 (Join-Path $tmp $asset)).Hash.ToLower()
        if ($want -ne $got) { Die "checksum mismatch for $asset (want $want, got $got)" }
        Say 'sha256 verified'

        Expand-Archive -LiteralPath (Join-Path $tmp $asset) -DestinationPath (Join-Path $tmp 'x') -Force
        Copy-Item -Force (Join-Path $tmp 'x\slopcop.exe') (Join-Path $Prefix 'slopcop.exe')
    }
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

$exe = Join-Path $Prefix 'slopcop.exe'
Say "installed $exe ($(& $exe --version))"

# Add the install folder to the user PATH once; nothing else is changed.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not $userPath) { $userPath = '' }
if (($userPath -split ';') -notcontains $Prefix) {
    [Environment]::SetEnvironmentVariable('Path', ($userPath.TrimEnd(';') + ";$Prefix").TrimStart(';'), 'User')
    Say "added $Prefix to your user PATH (open a new terminal to use 'slopcop')"
}
$env:Path = "$Prefix;$env:Path"
