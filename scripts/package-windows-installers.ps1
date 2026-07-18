param(
    [string]$OutDir = "dist",
    [switch]$RunLifecycleSmoke
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$PackagerVersion = "0.11.8"
$TargetTriple = "x86_64-pc-windows-msvc"
$ProductName = "OMENbrowser_rs"
$Identifier = "org.omensealed.omenbrowser"
$Publisher = "omensealed"

$ToolAssets = @(
    @{
        Name = "nsis-3.09.zip"
        Url = "https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.9/nsis-3.09.zip"
        Sha256 = "f5dc52eef1f3884230520199bac6f36b82d643d86b003ce51bd24b05c6ba7c91"
    },
    @{
        Name = "NSIS-ApplicationID.zip"
        Url = "https://github.com/tauri-apps/binary-releases/releases/download/nsis-plugins-v0/NSIS-ApplicationID.zip"
        Sha256 = "1c2772b0edfb0f96a7524734d6c8fac1fc011f26221faf88f3ed2c950f0c06c0"
    },
    @{
        Name = "nsis_tauri_utils.dll"
        Url = "https://github.com/tauri-apps/nsis-tauri-utils/releases/download/nsis_tauri_utils-v0.2.1/nsis_tauri_utils.dll"
        Sha256 = "0eed48313a7f904d7cc1977b70000ab3f11f18cadc8e6a69b807d288ca71f9db"
    },
    @{
        Name = "wix311-binaries.zip"
        Url = "https://github.com/wixtoolset/wix3/releases/download/wix3112rtm/wix311-binaries.zip"
        Sha256 = "2c1888d5d1dba377fc7fa14444cf556963747ff9a0a289a3599cf09da03b9e2e"
    }
)

function Invoke-Checked {
    param([string]$FilePath, [string[]]$Arguments)

    & $FilePath @Arguments | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath failed with exit code $LASTEXITCODE"
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

function Convert-ToMsiVersion {
    param([string]$Version)

    if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)-(\d+)$') {
        throw "release version is not numeric revision SemVer: $Version"
    }
    $parts = @($Matches[1], $Matches[2], $Matches[3], $Matches[4]) |
        ForEach-Object { [uint64]$_ }
    if ($parts[0] -gt 255 -or $parts[1] -gt 255 -or
        $parts[2] -gt 65535 -or $parts[3] -gt 65535) {
        throw "release version exceeds MSI numeric bounds: $Version"
    }
    return "$($parts[0]).$($parts[1]).$($parts[2]).$($parts[3])"
}

function Get-PriorRevisionVersion {
    param([string]$Version)

    if ($Version -notmatch '^(\d+\.\d+\.\d+)-(\d+)$') {
        throw "release version is not numeric revision SemVer: $Version"
    }
    $revision = [uint64]$Matches[2]
    if ($revision -eq 0) {
        throw "release revision must be greater than zero for upgrade qualification"
    }
    return "$($Matches[1])-$($revision - 1)"
}

function Write-Sha256 {
    param([string]$Path)

    $file = Get-Item -LiteralPath $Path
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $file.FullName).Hash.ToLowerInvariant()
    Set-Content -Encoding utf8 -LiteralPath "$($file.FullName).sha256" `
        -Value "$hash  $($file.Name)"
}

function Get-VerifiedAsset {
    param([hashtable]$Asset, [string]$DestinationDirectory)

    $destination = Join-Path $DestinationDirectory $Asset.Name
    Invoke-WebRequest -UseBasicParsing -Uri $Asset.Url -OutFile $destination
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $destination).Hash.ToLowerInvariant()
    if ($actual -ne $Asset.Sha256) {
        throw "SHA-256 mismatch for $($Asset.Name): expected=$($Asset.Sha256) actual=$actual"
    }
    return $destination
}

function Initialize-PackagerTools {
    param([string]$TemporaryDirectory)

    if (-not $env:LOCALAPPDATA) {
        throw "LOCALAPPDATA is required to seed cargo-packager tools"
    }
    $downloads = Join-Path $TemporaryDirectory "downloads"
    $extract = Join-Path $TemporaryDirectory "extract"
    $tools = Join-Path $env:LOCALAPPDATA ".cargo-packager"
    New-Item -ItemType Directory -Force -Path $downloads, $extract, $tools | Out-Null

    $resolved = @{}
    foreach ($asset in $ToolAssets) {
        $resolved[$asset.Name] = Get-VerifiedAsset $asset $downloads
    }

    $nsisExtract = Join-Path $extract "nsis"
    Expand-Archive -LiteralPath $resolved["nsis-3.09.zip"] -DestinationPath $nsisExtract -Force
    $nsisDestination = Join-Path $tools "NSIS"
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $nsisDestination
    Move-Item -LiteralPath (Join-Path $nsisExtract "nsis-3.09") -Destination $nsisDestination

    $applicationIdExtract = Join-Path $extract "application-id"
    Expand-Archive -LiteralPath $resolved["NSIS-ApplicationID.zip"] `
        -DestinationPath $applicationIdExtract -Force
    $pluginDirectory = Join-Path $nsisDestination "Plugins/x86-unicode"
    New-Item -ItemType Directory -Force -Path $pluginDirectory | Out-Null
    Copy-Item -LiteralPath (Join-Path $applicationIdExtract "ReleaseUnicode/ApplicationID.dll") `
        -Destination (Join-Path $pluginDirectory "ApplicationID.dll")
    Copy-Item -LiteralPath $resolved["nsis_tauri_utils.dll"] `
        -Destination (Join-Path $pluginDirectory "nsis_tauri_utils.dll")

    $wixDestination = Join-Path $tools "WixTools"
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $wixDestination
    Expand-Archive -LiteralPath $resolved["wix311-binaries.zip"] `
        -DestinationPath $wixDestination -Force

    foreach ($required in @(
        (Join-Path $nsisDestination "makensis.exe"),
        (Join-Path $pluginDirectory "ApplicationID.dll"),
        (Join-Path $pluginDirectory "nsis_tauri_utils.dll"),
        (Join-Path $wixDestination "candle.exe"),
        (Join-Path $wixDestination "light.exe")
    )) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "seeded cargo-packager tool is missing: $required"
        }
    }
}

function Write-PackagerConfig {
    param(
        [string]$Path,
        [string]$Version,
        [string]$PackageOut,
        [string]$BinariesDirectory
    )

    $config = [ordered]@{
        name = "omenbrowser-installer"
        productName = $ProductName
        version = $Version
        identifier = $Identifier
        formats = @("nsis", "wix")
        outDir = $PackageOut
        binariesDir = $BinariesDirectory
        targetTriple = $TargetTriple
        description = "OMENbrowser Rust desktop client"
        homepage = "https://github.com/omensealed/omenbrowser"
        publisher = $Publisher
        binaries = @([ordered]@{ path = "omenbrowser_rs"; main = $true })
        windows = [ordered]@{ allowDowngrades = $false }
        nsis = [ordered]@{ installMode = "currentUser"; languages = @("English") }
        wix = [ordered]@{ languages = @("en-US") }
    }
    $config | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 -LiteralPath $Path
}

function Invoke-Packager {
    param(
        [string]$Version,
        [string]$Destination,
        [string]$BinariesDirectory,
        [string]$TemporaryDirectory
    )

    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    $config = Join-Path $TemporaryDirectory "packager-$($Version.Replace('.', '-')).json"
    Write-PackagerConfig $config $Version $Destination $BinariesDirectory
    Invoke-Checked "cargo" @("packager", "--config", $config)

    $setup = @(Get-ChildItem -LiteralPath $Destination -Filter "*.exe" -File)
    $msi = @(Get-ChildItem -LiteralPath $Destination -Filter "*.msi" -File)
    if ($setup.Count -ne 1 -or $msi.Count -ne 1) {
        throw "unexpected installer counts for ${Version}: exe=$($setup.Count) msi=$($msi.Count)"
    }
    return [ordered]@{ Setup = $setup[0].FullName; Msi = $msi[0].FullName }
}

function Read-MsiProperty {
    param([string]$MsiPath, [string]$Property)

    $installer = New-Object -ComObject WindowsInstaller.Installer
    $database = $installer.GetType().InvokeMember(
        "OpenDatabase", "InvokeMethod", $null, $installer, @($MsiPath, 0)
    )
    $query = "SELECT ``Value`` FROM ``Property`` WHERE ``Property``='$Property'"
    $view = $database.GetType().InvokeMember(
        "OpenView", "InvokeMethod", $null, $database, @($query)
    )
    $view.GetType().InvokeMember("Execute", "InvokeMethod", $null, $view, $null) | Out-Null
    $record = $view.GetType().InvokeMember("Fetch", "InvokeMethod", $null, $view, $null)
    if ($null -eq $record) {
        throw "MSI property not found: $Property"
    }
    return $record.GetType().InvokeMember("StringData", "GetProperty", $null, $record, 1)
}

function Invoke-ProcessAndWait {
    param([string]$FilePath, [string[]]$Arguments)

    $process = Start-Process -FilePath $FilePath -ArgumentList $Arguments -PassThru -Wait
    if ($process.ExitCode -ne 0) {
        throw "$FilePath exited with code $($process.ExitCode)"
    }
}

function Get-UninstallDisplayVersion {
    param(
        [string[]]$Roots,
        [string]$ExpectedProductName
    )

    foreach ($root in $Roots) {
        if (-not (Test-Path -LiteralPath $root)) {
            continue
        }
        foreach ($entry in Get-ChildItem -LiteralPath $root) {
            $properties = Get-ItemProperty -LiteralPath $entry.PSPath
            $displayName = $properties.PSObject.Properties["DisplayName"]
            $displayVersion = $properties.PSObject.Properties["DisplayVersion"]
            if ($null -ne $displayName -and $displayName.Value -eq $ExpectedProductName -and
                $null -ne $displayVersion) {
                return [string]$displayVersion.Value
            }
        }
    }
    throw "installed product registration was not found: $ExpectedProductName"
}

function Assert-InstalledIdentity {
    param([string]$Binary, [string]$Version)

    if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
        throw "browser binary is missing: $Binary"
    }
    $identity = (& $Binary --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $identity.Contains("OMENbrowser_rs $Version")) {
        throw "browser identity mismatch: $identity"
    }
}

function Test-GuiLaunch {
    param([string]$Binary, [string]$AppRoot)

    $process = Start-Process -FilePath $Binary -ArgumentList @("--app-root", $AppRoot) -PassThru
    Start-Sleep -Seconds 4
    if ($process.HasExited) {
        throw "installed GUI exited during launch smoke with code $($process.ExitCode)"
    }
    Stop-Process -Id $process.Id -Force
    $process.WaitForExit()
}

function Test-InstallerLifecycle {
    param(
        [System.Collections.IDictionary]$Prior,
        [System.Collections.IDictionary]$Current,
        [string]$Version,
        [string]$PriorVersion,
        [string]$MsiVersion,
        [string]$PriorMsiVersion,
        [string]$TemporaryDirectory
    )

    $appRoot = Join-Path $TemporaryDirectory "isolated-user-data"
    New-Item -ItemType Directory -Force -Path $appRoot | Out-Null
    $sentinel = Join-Path $appRoot "preserve-after-uninstall.txt"
    Set-Content -Encoding utf8 -LiteralPath $sentinel `
        -Value "installer lifecycle must preserve user data"

    $nsisInstall = Join-Path $env:LOCALAPPDATA "$ProductName/omenbrowser_rs.exe"
    Invoke-ProcessAndWait $Prior.Setup @("/S")
    Assert-InstalledIdentity $nsisInstall $Version
    $nsisRegistry = @("HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall")
    if ((Get-UninstallDisplayVersion $nsisRegistry $ProductName) -ne $PriorVersion) {
        throw "NSIS prior-revision registration mismatch"
    }
    Invoke-ProcessAndWait $Current.Setup @("/S")
    Assert-InstalledIdentity $nsisInstall $Version
    if ((Get-UninstallDisplayVersion $nsisRegistry $ProductName) -ne $Version) {
        throw "NSIS upgrade registration mismatch"
    }
    Test-GuiLaunch $nsisInstall $appRoot
    Invoke-ProcessAndWait (Join-Path $env:LOCALAPPDATA "$ProductName/uninstall.exe") @("/S")
    Start-Sleep -Seconds 2
    if (Test-Path -LiteralPath $nsisInstall) {
        throw "NSIS uninstall retained the installed browser binary"
    }
    if (-not (Test-Path -LiteralPath $sentinel -PathType Leaf)) {
        throw "NSIS uninstall removed isolated user data"
    }

    $msiInstall = Join-Path $env:ProgramFiles "$ProductName/omenbrowser_rs.exe"
    Invoke-ProcessAndWait "msiexec.exe" @("/i", $Prior.Msi, "/qn", "/norestart")
    Assert-InstalledIdentity $msiInstall $Version
    $msiRegistry = @(
        "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall",
        "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"
    )
    if ((Get-UninstallDisplayVersion $msiRegistry $ProductName) -ne $PriorMsiVersion) {
        throw "MSI prior-revision registration mismatch"
    }
    Invoke-ProcessAndWait "msiexec.exe" @("/i", $Current.Msi, "/qn", "/norestart")
    Assert-InstalledIdentity $msiInstall $Version
    if ((Get-UninstallDisplayVersion $msiRegistry $ProductName) -ne $MsiVersion) {
        throw "MSI upgrade registration mismatch"
    }
    Test-GuiLaunch $msiInstall $appRoot
    Invoke-ProcessAndWait "msiexec.exe" @("/x", $Current.Msi, "/qn", "/norestart")
    if (Test-Path -LiteralPath $msiInstall) {
        throw "MSI uninstall retained the installed browser binary"
    }
    if (-not (Test-Path -LiteralPath $sentinel -PathType Leaf)) {
        throw "MSI uninstall removed isolated user data"
    }
}

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $repoRoot

$version = Read-PackageVersion "Cargo.toml"
$serverVersion = Read-PackageVersion "src/server/Cargo.toml"
if ($version -ne $serverVersion) {
    throw "package version mismatch: browser=$version server=$serverVersion"
}
$msiVersion = Convert-ToMsiVersion $version
$priorVersion = Get-PriorRevisionVersion $version
$priorMsiVersion = Convert-ToMsiVersion $priorVersion

$packagerIdentity = (& cargo packager --version | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or
    $packagerIdentity -notmatch "cargo-packager $([regex]::Escape($PackagerVersion))") {
    throw "expected cargo-packager $PackagerVersion, found: $packagerIdentity"
}

$browserBinary = Join-Path $repoRoot "target/release/omenbrowser_rs.exe"
Assert-InstalledIdentity $browserBinary $version

$resolvedOut = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutDir))
$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) "omenbrowser-installer-$PID"
New-Item -ItemType Directory -Force -Path $resolvedOut, $temporaryRoot | Out-Null
Initialize-PackagerTools $temporaryRoot

$prior = Invoke-Packager $priorVersion (Join-Path $temporaryRoot "prior") `
    (Split-Path -Parent $browserBinary) $temporaryRoot
$current = Invoke-Packager $version (Join-Path $temporaryRoot "current") `
    (Split-Path -Parent $browserBinary) $temporaryRoot

if ((Read-MsiProperty $current.Msi "ProductVersion") -ne $msiVersion) {
    throw "MSI ProductVersion does not match mapping $version -> $msiVersion"
}
if ((Read-MsiProperty $current.Msi "ProductName") -ne $ProductName) {
    throw "MSI ProductName mismatch"
}

if ($RunLifecycleSmoke) {
    Test-InstallerLifecycle $prior $current $version $priorVersion $msiVersion `
        $priorMsiVersion $temporaryRoot
}

$setupOut = Join-Path $resolvedOut "$ProductName-$version-windows-x86_64-setup-unsigned.exe"
$msiOut = Join-Path $resolvedOut "$ProductName-$version-windows-x86_64-unsigned.msi"
Copy-Item -Force -LiteralPath $current.Setup -Destination $setupOut
Copy-Item -Force -LiteralPath $current.Msi -Destination $msiOut

foreach ($installer in @($setupOut, $msiOut)) {
    $signature = Get-AuthenticodeSignature -LiteralPath $installer
    if ($signature.Status -ne "NotSigned") {
        throw "expected unsigned installer, found $($signature.Status): $installer"
    }
    Write-Sha256 $installer
}

Write-Host "Windows installers:"
Write-Host "  $setupOut"
Write-Host "  $msiOut"
Write-Host "MSI version mapping: $version -> $msiVersion"
Write-Host "Lifecycle smoke: $RunLifecycleSmoke"
