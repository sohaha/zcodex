[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$Version = 'latest',
    [string]$InstallDir = $(if ($env:CODEX_INSTALL_DIR) { $env:CODEX_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\zcodex\bin' })
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$BaseUrl = $env:CODEX_BASE_URL

function Write-Step {
    param([string]$Message)
    Write-Host "==> $Message"
}

function Normalize-Version {
    param([string]$RawVersion)

    if ([string]::IsNullOrWhiteSpace($RawVersion) -or $RawVersion -eq 'latest') {
        return 'latest'
    }

    if ($RawVersion.StartsWith('v')) {
        return $RawVersion.Substring(1)
    }

    return $RawVersion
}

function Resolve-Version {
    $normalized = Normalize-Version -RawVersion $Version
    if ($normalized -ne 'latest') {
        return $normalized
    }

    $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/sohaha/zcodex/releases/latest'
    if (-not $release.tag_name) {
        throw 'Failed to resolve the latest zcodex release version.'
    }

    return (Normalize-Version -RawVersion $release.tag_name)
}

function Get-Architecture {
    $runtimeInfo = 'System.Runtime.InteropServices.RuntimeInformation' -as [type]
    if ($runtimeInfo -and $runtimeInfo::OSArchitecture) {
        return [string]$runtimeInfo::OSArchitecture
    }

    switch ($env:PROCESSOR_ARCHITECTURE) {
        'ARM64' { return 'Arm64' }
        'AMD64' { return 'X64' }
        default { throw "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
    }
}

function Path-Contains {
    param(
        [string]$PathValue,
        [string]$Entry
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $false
    }

    $needle = $Entry.TrimEnd('\\')
    foreach ($segment in $PathValue.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries)) {
        if ($segment.TrimEnd('\\') -ieq $needle) {
            return $true
        }
    }

    return $false
}

function Release-Url-For-Asset {
    param(
        [string]$AssetName,
        [string]$ResolvedVersion
    )

    if (-not [string]::IsNullOrWhiteSpace($BaseUrl)) {
        return "{0}/{1}" -f $BaseUrl.TrimEnd('/'), $AssetName
    }

    return "https://github.com/sohaha/zcodex/releases/download/v{0}/{1}" -f $ResolvedVersion, $AssetName
}

function Download-File {
    param(
        [string]$Url,
        [string]$Destination
    )

    Invoke-WebRequest -Uri $Url -OutFile $Destination
}

function Download-First-Available-Asset {
    param(
        [string]$ResolvedVersion,
        [string[]]$AssetNames,
        [string]$Destination
    )

    foreach ($assetName in $AssetNames) {
        $url = Release-Url-For-Asset -AssetName $assetName -ResolvedVersion $ResolvedVersion
        try {
            Invoke-WebRequest -Uri $url -Method Head | Out-Null
            Download-File -Url $url -Destination $Destination
            return $assetName
        } catch {
            continue
        }
    }

    return $null
}

function Copy-Installed-File {
    param(
        [string]$SourcePath,
        [string]$DestinationPath
    )

    if (-not (Test-Path -LiteralPath $SourcePath -PathType Leaf)) {
        return $false
    }

    Copy-Item -LiteralPath $SourcePath -Destination $DestinationPath -Force
    return $true
}

function Install-From-NpmPackage {
    param(
        [string]$ArchivePath,
        [string]$ExtractDir,
        [string]$VendorTarget,
        [string]$InstallDirPath
    )

    tar -xzf $ArchivePath -C $ExtractDir

    $codexDir = Join-Path $ExtractDir "package/vendor/$VendorTarget/codex"
    $pathDir = Join-Path $ExtractDir "package/vendor/$VendorTarget/path"
    $assets = @(
        @{ Source = (Join-Path $codexDir 'codex.exe'); Destination = 'codex.exe'; Required = $true },
        @{ Source = (Join-Path $codexDir 'codex-command-runner.exe'); Destination = 'codex-command-runner.exe'; Required = $false },
        @{ Source = (Join-Path $codexDir 'codex-windows-sandbox-setup.exe'); Destination = 'codex-windows-sandbox-setup.exe'; Required = $false },
        @{ Source = (Join-Path $pathDir 'rg.exe'); Destination = 'rg.exe'; Required = $false }
    )

    foreach ($asset in $assets) {
        $destination = Join-Path $InstallDirPath $asset.Destination
        if (-not (Copy-Installed-File -SourcePath $asset.Source -DestinationPath $destination) -and $asset.Required) {
            throw "Downloaded npm package does not contain required file '$($asset.Destination)'."
        }
    }
}

function Install-From-ZipBundle {
    param(
        [string]$ArchivePath,
        [string]$ExtractDir,
        [string]$InstallDirPath
    )

    tar -xf $ArchivePath -C $ExtractDir

    $required = Join-Path $ExtractDir 'codex.exe'
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw 'Downloaded archive does not contain codex.exe.'
    }

    foreach ($fileName in @('codex.exe', 'codex-command-runner.exe', 'codex-windows-sandbox-setup.exe', 'rg.exe')) {
        $source = Join-Path $ExtractDir $fileName
        if (Test-Path -LiteralPath $source -PathType Leaf) {
            Copy-Item -LiteralPath $source -Destination (Join-Path $InstallDirPath $fileName) -Force
        }
    }
}

function Install-Legacy-Binaries {
    param(
        [string]$ResolvedVersion,
        [Object[]]$Assets,
        [string]$InstallDirPath
    )

    foreach ($asset in $Assets) {
        $url = Release-Url-For-Asset -AssetName $asset.Name -ResolvedVersion $ResolvedVersion
        $destination = Join-Path $InstallDirPath $asset.Destination
        Write-Step "Downloading $($asset.Name)"
        Download-File -Url $url -Destination $destination
    }
}

function Test-IsPortableExecutable {
    param([string]$Path)

    $stream = [System.IO.File]::OpenRead($Path)
    try {
        if ($stream.Length -lt 2) {
            return $false
        }

        return ($stream.ReadByte() -eq 0x4D -and $stream.ReadByte() -eq 0x5A)
    } finally {
        $stream.Dispose()
    }
}

if ($env:OS -ne 'Windows_NT') {
    throw 'codex-install.ps1 supports Windows only.'
}

if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'zcodex requires a 64-bit version of Windows.'
}

$arch = Get-Architecture
switch ($arch) {
    'Arm64' {
        $target = 'aarch64-pc-windows-msvc'
        $npmTag = 'win32-arm64'
        $platformLabel = 'Windows (ARM64)'
    }
    'X64' {
        $target = 'x86_64-pc-windows-msvc'
        $npmTag = 'win32-x64'
        $platformLabel = 'Windows (x64)'
    }
    default {
        throw "Unsupported architecture: $arch"
    }
}

$resolvedVersion = Resolve-Version
$npmAsset = "codex-npm-$npmTag-$resolvedVersion.tgz"
$fallbackZipAsset = "codex-$target.exe.zip"
$legacyAssets = @(
    @{ Name = "codex-$target.exe"; Destination = 'codex.exe' },
    @{ Name = "codex-command-runner-$target.exe"; Destination = 'codex-command-runner.exe' },
    @{ Name = "codex-windows-sandbox-setup-$target.exe"; Destination = 'codex-windows-sandbox-setup.exe' }
)

Write-Step "Installing zcodex v$resolvedVersion"
Write-Step "Detected platform: $platformLabel"
Write-Step "Install directory: $InstallDir"

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("codex-install-" + [System.Guid]::NewGuid().ToString())
$extractDir = Join-Path $tempRoot 'extract'
$archivePath = Join-Path $tempRoot 'downloaded-asset'
New-Item -ItemType Directory -Force -Path $extractDir | Out-Null

try {
    Write-Step 'Downloading Codex CLI'
    $downloadedAsset = Download-First-Available-Asset -ResolvedVersion $resolvedVersion -AssetNames @($npmAsset) -Destination $archivePath
    if ($downloadedAsset) {
        Write-Step "Using release asset: $downloadedAsset"
        Install-From-NpmPackage -ArchivePath $archivePath -ExtractDir $extractDir -VendorTarget $target -InstallDirPath $InstallDir
    } else {
        $downloadedAsset = Download-First-Available-Asset -ResolvedVersion $resolvedVersion -AssetNames @($fallbackZipAsset) -Destination $archivePath
        if ($downloadedAsset) {
            Write-Step "Using fallback release asset: $downloadedAsset"
            Install-From-ZipBundle -ArchivePath $archivePath -ExtractDir $extractDir -InstallDirPath $InstallDir
        } else {
            Write-Step 'Falling back to legacy Windows release assets'
            Install-Legacy-Binaries -ResolvedVersion $resolvedVersion -Assets $legacyAssets -InstallDirPath $InstallDir
        }
    }
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not (Path-Contains -PathValue $userPath -Entry $InstallDir)) {
    $newUserPath = if ([string]::IsNullOrWhiteSpace($userPath)) { $InstallDir } else { "$InstallDir;$userPath" }
    if ($PSCmdlet.ShouldProcess('User PATH', "Add $InstallDir")) {
        [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
    }
    $env:Path = "$InstallDir;$env:Path"
    Write-Step 'Added install directory to user PATH. Open a new shell if needed.'
}

$codexPath = Join-Path $InstallDir 'codex.exe'
if (-not (Test-Path -LiteralPath $codexPath -PathType Leaf)) {
    throw "codex.exe was not installed to $InstallDir"
}

if (Test-IsPortableExecutable -Path $codexPath) {
    Write-Step "Verifying $codexPath"
    if ($PSCmdlet.ShouldProcess($codexPath, 'codex --version')) {
        & $codexPath --version
        if ($LASTEXITCODE -ne 0) {
            throw "codex --version failed with exit code $LASTEXITCODE"
        }

        & $codexPath ztldr languages | Out-Null
        if ($LASTEXITCODE -ne 0) {
            throw "codex ztldr languages failed with exit code $LASTEXITCODE"
        }
    }
} elseif (-not [string]::IsNullOrWhiteSpace($BaseUrl)) {
    Write-Step 'Skipping executable smoke test for custom CODEX_BASE_URL assets.'
} else {
    throw 'Downloaded codex.exe is not a valid Windows executable.'
}
