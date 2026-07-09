param(
    [string]$Out,
    [string]$BazelTarget,
    [string]$HelperRoot = $env:CODEX_PACKAGE_HELPER_ROOT,
    [ValidateSet('dev', 'release')]
    [string]$CargoProfile = 'dev',
    [string]$GithubEnv,
    [switch]$PrintShellEnv,
    [switch]$ExportEnvironment,
    [switch]$BootstrapOnly,
    [switch]$VerifySandbox,
    [string]$ExecPath,
    [switch]$ReleasePackageArchive,
    [string]$ZstdSource,
    [switch]$WindowsReleaseShapedMsvc,
    [switch]$WindowsMsvcHostPlatform,
    [string]$BuildMetadataJob
)

$ErrorActionPreference = 'Stop'

if ($PrintShellEnv) {
    # -ExportEnvironment is the supported PowerShell env handoff;
    # -PrintShellEnv remains as a legacy alias for parity with the shell script.
    $ExportEnvironment = $true
}

if ($ReleasePackageArchive) {
    if ($CargoProfile -ne 'release') {
        throw '-ReleasePackageArchive requires -CargoProfile release.'
    }
    if ([string]::IsNullOrWhiteSpace($ZstdSource)) {
        $Zstd = Get-Command zstd -ErrorAction SilentlyContinue
        if (-not $Zstd) {
            throw 'zstd is required for -ReleasePackageArchive unless -ZstdSource points at a materialized executable.'
        }
    } elseif (-not (Test-Path $ZstdSource -PathType Leaf)) {
        throw "-ZstdSource must point at a materialized executable: $ZstdSource"
    }
    throw '-ReleasePackageArchive packageArchive staging is blocked until the native Windows app-server package archive lane is implemented.'
}

if ($BootstrapOnly) {
    if ($ExportEnvironment) {
        throw '-BootstrapOnly cannot be combined with -ExportEnvironment.'
    }
    exit 0
}

$RepoRoot = if ($env:GITHUB_WORKSPACE) {
    $env:GITHUB_WORKSPACE
} else {
    Resolve-Path (Join-Path $PSScriptRoot '..\..')
}

function Get-DefaultTarget {
    if ($IsWindows) {
        if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') {
            return 'aarch64-pc-windows-msvc'
        }
        return 'x86_64-pc-windows-msvc'
    }
    throw '-BazelTarget is required outside Windows PowerShell staging.'
}

if ([string]::IsNullOrWhiteSpace($BazelTarget)) {
    $BazelTarget = Get-DefaultTarget
}

function Get-PlatformName([string]$Target) {
    switch ($Target) {
        'x86_64-pc-windows-msvc' { 'windows_amd64'; return }
        'aarch64-pc-windows-msvc' { 'windows_arm64'; return }
        default { throw "Unsupported -BazelTarget for PowerShell staging: $Target" }
    }
}

function Get-EntryPointName {
    'codex.exe'
}

function Invoke-HelperRootVerification {
    if ([string]::IsNullOrWhiteSpace($HelperRoot)) {
        throw '-HelperRoot or CODEX_PACKAGE_HELPER_ROOT is required.'
    }
    $ManifestPath = Join-Path (Join-Path $HelperRoot $BazelTarget) 'codex-package-helpers.json'
    if (-not (Test-Path $ManifestPath -PathType Leaf)) {
        throw "Missing Stage 5G helper manifest: $ManifestPath"
    }
    $env:PYTHONPATH = Join-Path $RepoRoot 'scripts'
    python -m codex_package.materialize_helpers `
        --target $BazelTarget `
        --output-root $HelperRoot `
        --verify-only 1>&2
    if ($LASTEXITCODE -ne 0) {
        throw 'Stage 5G helper verification failed.'
    }
}

function Get-BazelSeedRoot {
    if ($env:CODEX_GO_SDK_TEST_LAYOUT_ROOT) {
        return (Resolve-Path $env:CODEX_GO_SDK_TEST_LAYOUT_ROOT).Path
    }

    $PlatformName = Get-PlatformName $BazelTarget
    $Label = "//codex-rs/cli:codex_go_sdk_runtime_layout_$PlatformName"
    $BazelArgs = @('build', $Label)
    if ($WindowsMsvcHostPlatform) {
        $BazelArgs = @('--host_platform=//:local_windows_msvc') + $BazelArgs
    }
    bazel @BazelArgs

    $Metadata = bazel cquery --output=files $Label |
        Where-Object { $_ -like '*/codex-package.json' -or $_ -like '*\codex-package.json' } |
        Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($Metadata)) {
        throw "Unable to locate codex-package.json from Bazel target $Label"
    }
    return (Split-Path $Metadata -Parent)
}

function Copy-Helper([string]$Source, [string]$Destination) {
    if (-not (Test-Path $Source -PathType Leaf)) {
        throw "Missing materialized helper: $Source"
    }
    New-Item -ItemType Directory -Force -Path (Split-Path $Destination -Parent) | Out-Null
    Copy-Item -Path $Source -Destination $Destination -Force
}

function Test-StagedLayout([string]$Root) {
    foreach ($Required in @(
        'codex-package.json',
        'bin/codex.exe',
        'codex-path/rg.exe',
        'codex-resources/codex-command-runner.exe',
        'codex-resources/codex-windows-sandbox-setup.exe'
    )) {
        $Path = Join-Path $Root $Required
        if (-not (Test-Path $Path -PathType Leaf)) {
            throw "Missing staged runtime file: $Path"
        }
    }
}

function ConvertTo-PowerShellSingleQuoted([string]$Value) {
    "'" + $Value.Replace("'", "''") + "'"
}

function Write-StagingMetadata([string]$RuntimeSource, [string[]]$ArchiveFormats, [string]$ZstdSource = '') {
    $Metadata = [ordered]@{
        archiveFormats = $ArchiveFormats
        bazelTarget = $BazelTarget
        buildMetadataJob = $BuildMetadataJob
        cargoProfile = $CargoProfile
        codeExecPath = [System.IO.Path]::GetFullPath((Join-Path $Out 'bin/codex.exe'))
        layoutTarget = $BazelTarget
        runtimeSource = $RuntimeSource
        windowsMsvcHostPlatform = [bool]$WindowsMsvcHostPlatform
        windowsReleaseShapedMsvc = [bool]$WindowsReleaseShapedMsvc
        zstdSource = $ZstdSource
    }
    $MetadataPath = Join-Path $Out 'codex-go-sdk-runtime-staging.json'
    $Metadata | ConvertTo-Json -Depth 4 | Out-File -FilePath $MetadataPath -Encoding utf8
}

if ($VerifySandbox) {
    if ([string]::IsNullOrWhiteSpace($ExecPath)) {
        throw '-VerifySandbox requires -ExecPath.'
    }
    $PackageRoot = Split-Path (Split-Path $ExecPath -Parent) -Parent
    $Metadata = Get-Content -Raw -Encoding UTF8 (Join-Path $PackageRoot 'codex-package.json') | ConvertFrom-Json
    $BazelTarget = $Metadata.target
    Test-StagedLayout $PackageRoot
    exit 0
}

if ([string]::IsNullOrWhiteSpace($Out)) {
    throw '-Out is required.'
}

Invoke-HelperRootVerification
$SeedRoot = Get-BazelSeedRoot
$HelperTargetRoot = Join-Path $HelperRoot $BazelTarget

if (Test-Path $Out) {
    Remove-Item -Recurse -Force $Out
}
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Copy-Item -Recurse -Force (Join-Path $SeedRoot '*') $Out
New-Item -ItemType Directory -Force -Path (Join-Path $Out 'codex-resources') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $Out 'codex-path') | Out-Null

Copy-Helper (Join-Path $HelperTargetRoot 'rg.exe') (Join-Path $Out 'codex-path/rg.exe')
Copy-Helper (Join-Path $HelperTargetRoot 'codex-command-runner.exe') (Join-Path $Out 'codex-resources/codex-command-runner.exe')
Copy-Helper (Join-Path $HelperTargetRoot 'codex-windows-sandbox-setup.exe') (Join-Path $Out 'codex-resources/codex-windows-sandbox-setup.exe')

Test-StagedLayout $Out
Write-StagingMetadata 'bazelLayout' @()

$CodeExecPath = Join-Path $Out 'bin/codex.exe'
$CodeHome = Join-Path $Out 'codex-home'
New-Item -ItemType Directory -Force -Path $CodeHome | Out-Null
if ($GithubEnv) {
    "CODEX_EXEC_PATH=$CodeExecPath" | Out-File -FilePath $GithubEnv -Encoding utf8 -Append
}
if ($ExportEnvironment) {
    "Set-Item -Path Env:CODEX_EXEC_PATH -Value $(ConvertTo-PowerShellSingleQuoted $CodeExecPath)"
    "Set-Item -Path Env:CODEX_HOME -Value $(ConvertTo-PowerShellSingleQuoted $CodeHome)"
    "Set-Item -Path Env:CODEX_GO_SDK_RUNTIME_ROOT -Value $(ConvertTo-PowerShellSingleQuoted $Out)"
}
