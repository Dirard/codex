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

function Export-StagingEnv([string]$Name, [string]$Value) {
    Set-Item -Path "Env:$Name" -Value $Value
    if ($env:GITHUB_ENV) {
        "$Name=$Value" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
    }
}

function Import-GitHubEnvFile([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path $Path -PathType Leaf)) {
        return
    }
    foreach ($Line in Get-Content -Encoding UTF8 $Path) {
        if ($Line -notmatch '^(.*?)=(.*)$') {
            continue
        }
        Set-Item -Path "Env:$($Matches[1])" -Value $Matches[2]
    }
}

function Initialize-WindowsBazelBootstrap {
    if (-not $IsWindows) {
        return
    }
    if ($WindowsReleaseShapedMsvc -and -not $WindowsMsvcHostPlatform) {
        $script:WindowsMsvcHostPlatform = $true
    }

    $OriginalGithubEnv = $env:GITHUB_ENV
    if ([string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
        if ($GithubEnv) {
            $env:GITHUB_ENV = $GithubEnv
        } else {
            $env:GITHUB_ENV = Join-Path ([System.IO.Path]::GetTempPath()) "codex-go-sdk-stage-env-$PID.txt"
            New-Item -ItemType File -Force -Path $env:GITHUB_ENV | Out-Null
        }
    }

    $RepositoryCachePath = Join-Path $HOME '.cache/bazel-repo-cache'
    Export-StagingEnv 'BAZEL_REPOSITORY_CACHE' $RepositoryCachePath

    if ([string]::IsNullOrWhiteSpace($env:BAZEL_OUTPUT_USER_ROOT)) {
        $BazelOutputUserRoot = if (Test-Path 'D:\') { 'D:\b' } else { 'C:\b' }
        Export-StagingEnv 'BAZEL_OUTPUT_USER_ROOT' $BazelOutputUserRoot
    }
    if ([string]::IsNullOrWhiteSpace($env:BAZEL_REPO_CONTENTS_CACHE)) {
        $RunId = if ($env:GITHUB_RUN_ID) { $env:GITHUB_RUN_ID } else { 'local' }
        $JobName = if ($env:GITHUB_JOB) { $env:GITHUB_JOB } else { 'go-sdk-stage' }
        $TempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
        Export-StagingEnv 'BAZEL_REPO_CONTENTS_CACHE' (Join-Path $TempRoot "bazel-repo-contents-cache-$RunId-$JobName")
    }

    if ($WindowsMsvcHostPlatform) {
        & (Join-Path $RepoRoot '.github/actions/setup-msvc-env/setup-msvc-env.ps1') -Target $BazelTarget
        Import-GitHubEnvFile $env:GITHUB_ENV
    }

    & (Join-Path $RepoRoot '.github/scripts/compute-bazel-windows-path.ps1')
    Import-GitHubEnvFile $env:GITHUB_ENV
    if ([string]::IsNullOrWhiteSpace($env:CODEX_BAZEL_WINDOWS_PATH)) {
        throw 'CODEX_BAZEL_WINDOWS_PATH was not exported by compute-bazel-windows-path.ps1.'
    }
    git config --global core.longpaths true

    if ([string]::IsNullOrWhiteSpace($OriginalGithubEnv) -and -not $GithubEnv) {
        Remove-Item -Force $env:GITHUB_ENV -ErrorAction SilentlyContinue
        Remove-Item Env:GITHUB_ENV -ErrorAction SilentlyContinue
    }
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
        --verify-only
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
    $BazelArgs = @('build')
    if ($WindowsMsvcHostPlatform) {
        $BazelArgs += '--host_platform=//:local_windows_msvc'
    }
    $BazelArgs += $Label
    bazel @BazelArgs

    $BazelCqueryArgs = @('cquery')
    if ($WindowsMsvcHostPlatform) {
        $BazelCqueryArgs += '--host_platform=//:local_windows_msvc'
    }
    $BazelCqueryArgs += @('--output=files', $Label)
    $Metadata = bazel @BazelCqueryArgs |
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

function Copy-SeedBinary([string]$RelativePath, [string]$Description) {
    $Source = Join-Path $SeedRoot $RelativePath
    $Destination = Join-Path $Out $RelativePath
    if (-not (Test-Path $Source -PathType Leaf)) {
        throw "Bazel runtime seed $Description is missing: $Source"
    }
    $CopySource = $Source
    $SourceItem = Get-Item -LiteralPath $Source
    if ($SourceItem.LinkType) {
        $CopySource = $SourceItem.Target
        if (-not [System.IO.Path]::IsPathRooted($CopySource)) {
            $CopySource = Join-Path (Split-Path $Source) $CopySource
        }
    }
    New-Item -ItemType Directory -Force -Path (Split-Path $Destination -Parent) | Out-Null
    Copy-Item -LiteralPath $CopySource -Destination $Destination -Force
    $StagedItem = Get-Item -LiteralPath $Destination
    if ($StagedItem.LinkType) {
        throw "Staged runtime $Description must be a real executable, not a symlink: $Destination"
    }
}

function Test-StagedLayout([string]$Root) {
    foreach ($Required in @(
        'codex-package.json',
        'bin/codex.exe',
        'bin/codex-code-mode-host.exe',
        'codex-path/rg.exe',
        'codex-resources/codex-command-runner.exe',
        'codex-resources/codex-windows-sandbox-setup.exe'
    )) {
        $Path = Join-Path $Root $Required
        if (-not (Test-Path $Path -PathType Leaf)) {
            throw "Missing staged runtime file: $Path"
        }
    }
    foreach ($RuntimeBinary in @('bin/codex.exe', 'bin/codex-code-mode-host.exe')) {
        $Path = Join-Path $Root $RuntimeBinary
        if ((Get-Item -LiteralPath $Path).LinkType) {
            throw "Staged runtime binary must be a real executable, not a symlink: $Path"
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
Initialize-WindowsBazelBootstrap
$SeedRoot = Get-BazelSeedRoot
$HelperTargetRoot = Join-Path $HelperRoot $BazelTarget

if (Test-Path $Out) {
    Remove-Item -Recurse -Force $Out
}
New-Item -ItemType Directory -Force -Path $Out | Out-Null
Copy-Item -Recurse -Force (Join-Path $SeedRoot '*') $Out
Copy-SeedBinary 'bin/codex.exe' 'entrypoint'
Copy-SeedBinary 'bin/codex-code-mode-host.exe' 'code-mode host'
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
