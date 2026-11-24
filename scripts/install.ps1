# One-line installer for bside (Windows):
#   irm https://raw.githubusercontent.com/002love/bside/main/scripts/install.ps1 | iex
#
# Optional environment variables:
#   $env:BSIDE_VERSION      Release tag to install (e.g. v0.1.0). Defaults to latest release.
#   $env:BSIDE_INSTALL_DIR  Destination directory for bside.exe. Defaults to "$env:LOCALAPPDATA\bside\bin".

$ErrorActionPreference = 'Stop'

try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch {
    # Ignore on newer PowerShell / .NET runtimes where TLS 1.2+ is managed automatically.
}

$Repo = '002love/bside'
$BinaryName = 'bside.exe'
$InstallDir = if ($env:BSIDE_INSTALL_DIR) { $env:BSIDE_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'bside\bin' }

function Resolve-BsideVersion {
    if ($env:BSIDE_VERSION) {
        $ver = $env:BSIDE_VERSION.Trim()
        if ($ver -notmatch '^v') {
            $ver = "v$ver"
        }
        return $ver
    }

    $apiUrl = "https://api.github.com/repos/$Repo/releases/latest"
    $headers = @{
        'Accept'     = 'application/vnd.github+json'
        'User-Agent' = 'bside-installer'
    }
    $release = Invoke-RestMethod -Uri $apiUrl -Headers $headers
    if (-not $release.tag_name) {
        throw "Could not determine latest release tag from $apiUrl"
    }
    return $release.tag_name
}

function Verify-Sha256Checksum {
    param(
        [Parameter(Mandatory = $true)][string]$ArchivePath,
        [Parameter(Mandatory = $true)][string]$ArchiveName,
        [Parameter(Mandatory = $true)][string]$SumsPath
    )

    $expectedHash = $null
    foreach ($line in Get-Content -Path $SumsPath) {
        $trimmed = $line.Trim()
        if (-not $trimmed) { continue }
        if ($trimmed -match '^([0-9a-fA-F]{64})\s+\*?(.+)$') {
            $hash = $Matches[1].ToLowerInvariant()
            $file = $Matches[2].Trim()
            if ($file -eq $ArchiveName) {
                $expectedHash = $hash
                break
            }
        }
    }

    if (-not $expectedHash) {
        throw "Checksum for $ArchiveName not found in SHA256SUMS.txt"
    }

    $actualHash = (Get-FileHash -Path $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $expectedHash) {
        throw "SHA256 checksum mismatch for $ArchiveName (expected $expectedHash, got $actualHash)"
    }
}

function Add-ToUserPath {
    param(
        [Parameter(Mandatory = $true)][string]$Directory
    )

    $normalizedTarget = $Directory.TrimEnd('\')
    $currentUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @()
    if ($currentUserPath) {
        $entries = $currentUserPath -split ';' | Where-Object { $_ -and $_.Trim() -ne '' }
    }

    $alreadyPresent = $false
    foreach ($entry in $entries) {
        if ($entry.Trim().TrimEnd('\') -ieq $normalizedTarget) {
            $alreadyPresent = $true
            break
        }
    }

    if (-not $alreadyPresent) {
        $newUserPath = if ($entries.Count -gt 0) {
            ($entries + $Directory) -join ';'
        } else {
            $Directory
        }
        [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
        Write-Host "Added $Directory to your User PATH."
    }

    $sessionEntries = ($env:PATH -split ';') | Where-Object { $_ -and $_.Trim() -ne '' }
    $inSession = $false
    foreach ($entry in $sessionEntries) {
        if ($entry.Trim().TrimEnd('\') -ieq $normalizedTarget) {
            $inSession = $true
            break
        }
    }
    if (-not $inSession) {
        $env:PATH = "$Directory;$env:PATH"
    }
}

$tag = Resolve-BsideVersion
$archiveName = "bside-$tag-x86_64-windows.zip"
$baseUrl = "https://github.com/$Repo/releases/download/$tag"
$archiveUrl = "$baseUrl/$archiveName"
$sumsUrl = "$baseUrl/SHA256SUMS.txt"

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("bside-install-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tempDir | Out-Null

try {
    Write-Host "Installing bside $tag (x86_64-windows)..."

    $archivePath = Join-Path $tempDir $archiveName
    $sumsPath = Join-Path $tempDir 'SHA256SUMS.txt'

    Invoke-WebRequest -Uri $archiveUrl -OutFile $archivePath -UseBasicParsing
    Invoke-WebRequest -Uri $sumsUrl -OutFile $sumsPath -UseBasicParsing

    Write-Host "Verifying SHA256 checksum..."
    Verify-Sha256Checksum -ArchivePath $archivePath -ArchiveName $archiveName -SumsPath $sumsPath

    $extractDir = Join-Path $tempDir 'extracted'
    Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force

    $extractedBinary = Join-Path $extractDir $BinaryName
    if (-not (Test-Path -Path $extractedBinary)) {
        throw "Expected binary '$BinaryName' not found inside $archiveName"
    }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $destPath = Join-Path $InstallDir $BinaryName
    Copy-Item -Path $extractedBinary -Destination $destPath -Force

    Add-ToUserPath -Directory $InstallDir

    Write-Host ""
    Write-Host "Installed bside $tag to $destPath"
    Write-Host ""
    Write-Host "Next steps (open a new terminal if 'bside' is not yet recognized):"
    Write-Host "  bside login"
    Write-Host "  bside service install"
    Write-Host "  bside watch"
} finally {
    if (Test-Path -Path $tempDir) {
        Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
