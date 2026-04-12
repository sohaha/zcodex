[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$Version = 'latest',
    [string]$InstallDir = $(Join-Path $env:LOCALAPPDATA 'Programs\OpenAI\Codex\bin')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

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
        $platformLabel = 'Windows (ARM64)'
    }
    'X64' {
        $target = 'x86_64-pc-windows-msvc'
        $platformLabel = 'Windows (x64)'
    }
    default {
        throw "Unsupported architecture: $arch"
    }
}

$resolvedVersion = Resolve-Version
$baseUrl = "https://github.com/sohaha/zcodex/releases/download/v$resolvedVersion"
$PathInstallDir = Join-Path $env:LOCALAPPDATA 'Programs\zcodex\bin'
$assets = @(
    @{ Name = "codex-$target.exe"; Destination = 'codex.exe' },
    @{ Name = "codex-command-runner-$target.exe"; Destination = 'codex-command-runner.exe' },
    @{ Name = "codex-windows-sandbox-setup-$target.exe"; Destination = 'codex-windows-sandbox-setup.exe' }
)

Write-Step "Installing zcodex v$resolvedVersion"
Write-Step "Detected platform: $platformLabel"
Write-Step "Install directory: $InstallDir"
Write-Step "PATH directory: $PathInstallDir"

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $PathInstallDir | Out-Null

foreach ($asset in $assets) {
    $url = "$baseUrl/$($asset.Name)"
    $destination = Join-Path $InstallDir $asset.Destination
    Write-Step "Downloading $($asset.Name)"
    if ($PSCmdlet.ShouldProcess($destination, "Download $url")) {
        Invoke-WebRequest -Uri $url -OutFile $destination
    }

    $pathDestination = Join-Path $PathInstallDir $asset.Destination
    if ($PSCmdlet.ShouldProcess($pathDestination, "Copy $destination")) {
        Copy-Item -Force $destination $pathDestination
    }
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not (Path-Contains -PathValue $userPath -Entry $PathInstallDir)) {
    $newUserPath = if ([string]::IsNullOrWhiteSpace($userPath)) { $PathInstallDir } else { "$PathInstallDir;$userPath" }
    if ($PSCmdlet.ShouldProcess('User PATH', "Add $PathInstallDir")) {
        [Environment]::SetEnvironmentVariable('Path', $newUserPath, 'User')
    }
    $env:Path = "$PathInstallDir;$env:Path"
    Write-Step 'Added PATH directory to user PATH. Open a new shell if needed.'
}

$codexPath = Join-Path $PathInstallDir 'codex.exe'
Write-Step "Verifying $codexPath"
if ($PSCmdlet.ShouldProcess($codexPath, 'codex --version')) {
    & $codexPath --version
    if ($LASTEXITCODE -ne 0) {
        throw "codex --version failed with exit code $LASTEXITCODE"
    }
}
