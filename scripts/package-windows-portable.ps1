param(
    [string]$OutDir = "dist"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-Cargo {
    param([string[]]$Arguments)

    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo command failed with exit code $LASTEXITCODE"
    }
}

function Read-PackageVersion {
    param([string]$ManifestPath)

    $inPackage = $false
    foreach ($line in Get-Content -LiteralPath $ManifestPath) {
        $trimmed = $line.Trim()
        if ($trimmed -eq "[package]") {
            $inPackage = $true
            continue
        }
        if ($inPackage -and $trimmed.StartsWith("[")) {
            break
        }
        if ($inPackage -and $trimmed -match '^version\s*=\s*"([^"]+)"') {
            return $Matches[1]
        }
    }
    throw "package version not found in $ManifestPath"
}

function Write-Sha256 {
    param([string]$ArchivePath)

    $archive = Get-Item -LiteralPath $ArchivePath
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive.FullName).Hash.ToLowerInvariant()
    Set-Content -Encoding utf8 -LiteralPath "$($archive.FullName).sha256" -Value "$hash  $($archive.Name)"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $repoRoot

$version = Read-PackageVersion "Cargo.toml"
$serverVersion = Read-PackageVersion "src/server/Cargo.toml"
if ($serverVersion -ne $version) {
    throw "package version mismatch: browser=$version server=$serverVersion"
}

$rustcIdentity = (& rustc -vV | Out-String)
if ($LASTEXITCODE -ne 0) {
    throw "rustc identity command failed with exit code $LASTEXITCODE"
}
$hostLine = @($rustcIdentity -split "\r?\n" | Where-Object { $_.StartsWith("host: ") })
if ($hostLine.Count -ne 1) {
    throw "rustc identity did not report exactly one host target"
}
$hostTarget = $hostLine[0].Substring("host: ".Length).Trim()
if ($hostTarget -ne "x86_64-pc-windows-msvc") {
    throw "unsupported Windows package host: $hostTarget"
}

Write-Host "== Building Windows desktop product =="
Invoke-Cargo @(
    "build", "--release", "--locked", "--no-default-features",
    "--features", "desktop-product", "--bin", "omenbrowser_rs"
)

Write-Host "== Building standalone Windows omenchatd =="
Invoke-Cargo @(
    "build", "--release", "--locked", "--manifest-path", "src/server/Cargo.toml",
    "--no-default-features", "--features", "server-full", "--bin", "omenchatd"
)

$browserBinary = Join-Path $repoRoot "target/release/omenbrowser_rs.exe"
$serverBinary = Join-Path $repoRoot "src/server/target/release/omenchatd.exe"
if (-not (Test-Path -LiteralPath $browserBinary -PathType Leaf)) {
    throw "missing browser release binary: $browserBinary"
}
if (-not (Test-Path -LiteralPath $serverBinary -PathType Leaf)) {
    throw "missing server release binary: $serverBinary"
}

$browserIdentity = (& $browserBinary --version | Out-String).Trim()
if ($LASTEXITCODE -ne 0) {
    throw "browser version command failed with exit code $LASTEXITCODE"
}
$serverIdentity = (& $serverBinary --version | Out-String).Trim()
if ($LASTEXITCODE -ne 0) {
    throw "omenchatd version command failed with exit code $LASTEXITCODE"
}

foreach ($required in @(
    "OMENbrowser_rs $version",
    "target=x86_64-pc-windows-msvc",
    "desktop-product:on",
    "native-network:on",
    "mock-runtime:off"
)) {
    if (-not $browserIdentity.Contains($required)) {
        throw "browser identity is missing: $required"
    }
}
foreach ($required in @(
    "omenchatd $version",
    "server-full:on",
    "live-reticulum:on"
)) {
    if (-not $serverIdentity.Contains($required)) {
        throw "omenchatd identity is missing: $required"
    }
}

$resolvedOut = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutDir))
New-Item -ItemType Directory -Force -Path $resolvedOut | Out-Null

$browserName = "OMENbrowser_rs-$version-windows-x86_64-portable"
$serverName = "omenchatd-$version-windows-x86_64"
$browserStage = Join-Path $resolvedOut $browserName
$serverStage = Join-Path $resolvedOut $serverName
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $browserStage, $serverStage
New-Item -ItemType Directory -Force -Path $browserStage, $serverStage | Out-Null

Copy-Item -LiteralPath $browserBinary -Destination (Join-Path $browserStage "omenbrowser_rs.exe")
Copy-Item -LiteralPath "README.md" -Destination $browserStage
Copy-Item -LiteralPath "TESTERS.md" -Destination $browserStage
Copy-Item -LiteralPath "docs/QUICKSTART.md" -Destination $browserStage
Copy-Item -LiteralPath "assets/fonts/adwaita/OFL.txt" -Destination (Join-Path $browserStage "ADWAITA_MONO_OFL.txt")
Set-Content -Encoding utf8 -LiteralPath (Join-Path $browserStage "PACKAGE-METADATA.txt") -Value @(
    "version: $version",
    "target: x86_64-pc-windows-msvc",
    "profile: desktop-product",
    "unsigned: true"
)

Copy-Item -LiteralPath $serverBinary -Destination (Join-Path $serverStage "omenchatd.exe")
Copy-Item -LiteralPath "src/server/README.md" -Destination $serverStage
Copy-Item -LiteralPath "docs/OMENCHAT_PROTOCOL.md" -Destination $serverStage
Set-Content -Encoding utf8 -LiteralPath (Join-Path $serverStage "PACKAGE-METADATA.txt") -Value @(
    "version: $version",
    "target: x86_64-pc-windows-msvc",
    "profile: server-full",
    "unsigned: true",
    "service_install: none"
)

$browserArchive = Join-Path $resolvedOut "$browserName.zip"
$serverArchive = Join-Path $resolvedOut "$serverName.zip"
Remove-Item -Force -ErrorAction SilentlyContinue $browserArchive, $serverArchive
Compress-Archive -CompressionLevel Optimal -LiteralPath $browserStage -DestinationPath $browserArchive
Compress-Archive -CompressionLevel Optimal -LiteralPath $serverStage -DestinationPath $serverArchive
Write-Sha256 $browserArchive
Write-Sha256 $serverArchive

Write-Host "Windows portable packages:"
Write-Host "  $browserArchive"
Write-Host "  $serverArchive"
