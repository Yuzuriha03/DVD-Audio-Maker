#requires -Version 5.1
<#
.SYNOPSIS
Records the provenance of one Android release APK.

.DESCRIPTION
Writes a release-build.json next to the APK (or to -OutputPath) containing the
source commit, application version, Rust/Android target, NDK, SDK levels, the
master/derived icon SHA-256 values and the APK SHA-256. The versionName/
versionCode and SDK levels are read back from the finished APK with aapt2 and
are checked against the pubspec/Gradle expectations, so the record describes
the artifact that was actually built. The record states that the APK was only
built: a device run is not claimed. tools/android/build-flutter.ps1 is the only
caller.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ApkPath,
    [Parameter(Mandatory = $true)][string]$RepoRoot,
    [string]$OutputPath = '',
    [string]$RustTarget = 'aarch64-linux-android',
    [string]$AndroidTarget = 'android-arm64',
    [int]$CompileSdk = 37,
    [int]$MinSdk = 24,
    [int]$TargetSdk = 37,
    [string]$AndroidSdkRoot = ''
)

$ErrorActionPreference = 'Stop'

function Get-SourceCommit {
    param([string]$Root)
    $record = [ordered]@{ commit = 'unknown'; worktreeDirty = $null }
    # Concurrent builds and review agents may hold the index; optional locks plus
    # one retry keep a transient git failure from losing the provenance commit.
    $previousLocks = $env:GIT_OPTIONAL_LOCKS
    $env:GIT_OPTIONAL_LOCKS = '0'
    try {
        for ($attempt = 0; $attempt -lt 2; $attempt++) {
            $commit = & git -C $Root rev-parse --verify HEAD 2>&1
            if ($LASTEXITCODE -eq 0 -and $commit) {
                $record.commit = ("$commit" | Select-Object -First 1).Trim()
                break
            }
            Start-Sleep -Milliseconds 200
        }
        if ($record.commit -ne 'unknown') {
            $status = & git -C $Root status --porcelain 2>&1
            if ($LASTEXITCODE -eq 0) {
                $record.worktreeDirty = [bool]$status
            }
        }
    } catch {
        return $record
    } finally {
        if ($null -eq $previousLocks) {
            Remove-Item Env:GIT_OPTIONAL_LOCKS -ErrorAction SilentlyContinue
        } else {
            $env:GIT_OPTIONAL_LOCKS = $previousLocks
        }
    }
    return $record
}

function Get-ApplicationVersion {
    param([string]$Root)
    $record = [ordered]@{ versionName = 'unknown'; versionCode = $null }
    $pubspec = Join-Path $Root 'apps/android/pubspec.yaml'
    if (-not (Test-Path -LiteralPath $pubspec)) {
        return $record
    }
    $match = [regex]::Match((Get-Content -LiteralPath $pubspec -Raw), '(?m)^version:\s*(\S+)')
    if (-not $match.Success) {
        return $record
    }
    $parts = $match.Groups[1].Value.Split('+', 2)
    $record.versionName = $parts[0]
    if ($parts.Count -gt 1) {
        $code = 0
        if ([int]::TryParse($parts[1], [ref]$code)) {
            $record.versionCode = $code
        }
    }
    return $record
}

function Get-NdkRecord {
    param([string]$Root)
    # Only a directory that really carries source.properties is a usable NDK;
    # a stale environment variable must not end up in the provenance record.
    $candidates = @(
        $env:ANDROID_NDK_ROOT,
        $env:DVDA_ANDROID_NDK,
        (Join-Path $Root 'build/android-ndk-r30')
    )
    $rejected = @()
    foreach ($candidate in $candidates) {
        if (-not $candidate) { continue }
        $properties = Join-Path $candidate 'source.properties'
        if (-not (Test-Path -LiteralPath $properties -PathType Leaf)) {
            $rejected += $candidate
            continue
        }
        $revision = 'unknown'
        $match = [regex]::Match((Get-Content -LiteralPath $properties -Raw), '(?m)^Pkg\.Revision\s*=\s*(\S+)')
        if ($match.Success) { $revision = $match.Groups[1].Value }
        return [ordered]@{ root = $candidate; revision = $revision; rejected = $rejected }
    }
    return [ordered]@{ root = $null; revision = 'unknown'; rejected = $rejected }
}

function Get-FlutterRecord {
    param([string]$Root)
    $record = [ordered]@{ version = 'unknown'; channel = 'unknown'; revision = 'unknown'; root = $null }
    $metadata = Join-Path $Root 'build/flutter/bin/cache/flutter.version.json'
    if (Test-Path -LiteralPath $metadata) {
        try {
            $json = Get-Content -LiteralPath $metadata -Raw | ConvertFrom-Json
            $record.version = if ($json.frameworkVersion) { $json.frameworkVersion } else { 'unknown' }
            $record.channel = if ($json.channel) { $json.channel } else { 'unknown' }
            $record.revision = if ($json.frameworkRevision) { $json.frameworkRevision } else { 'unknown' }
            $record.root = 'build/flutter'
            return $record
        } catch {
            return $record
        }
    }
    $versionFile = Join-Path $Root 'build/flutter/version'
    if (Test-Path -LiteralPath $versionFile) {
        $text = (Get-Content -LiteralPath $versionFile -Raw).Trim()
        if ($text) {
            $record.version = $text
            $record.root = 'build/flutter'
        }
    }
    return $record
}

function Get-Aapt2Path {
    param([string]$Root, [string]$SdkRoot)
    if ($env:AAPT2 -and (Test-Path -LiteralPath $env:AAPT2 -PathType Leaf)) {
        return $env:AAPT2
    }
    $sdkRoots = @()
    if ($SdkRoot) { $sdkRoots += $SdkRoot }
    foreach ($candidate in @($env:ANDROID_HOME, $env:ANDROID_SDK_ROOT)) {
        if ($candidate) { $sdkRoots += $candidate }
    }
    $sdkRoots += (Join-Path $Root 'build/android-sdk')
    foreach ($sdk in ($sdkRoots | Select-Object -Unique)) {
        $buildTools = Join-Path $sdk 'build-tools'
        if (-not (Test-Path -LiteralPath $buildTools -PathType Container)) { continue }
        $directories = @(Get-ChildItem -LiteralPath $buildTools -Directory | Sort-Object -Property Name -Descending)
        foreach ($directory in $directories) {
            $candidate = Join-Path $directory.FullName 'aapt2.exe'
            if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
        }
    }
    $command = Get-Command aapt2.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    return $null
}

function Read-ApkBadging {
    param([string]$Aapt2, [string]$Apk)
    $output = & $Aapt2 dump badging $Apk 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "aapt2 dump badging failed for $Apk : $($output | Select-Object -Last 5)"
    }
    $text = $output | Out-String
    $record = [ordered]@{
        packageName = $null
        versionName = $null
        versionCode = $null
        compileSdk = $null
        minSdk = $null
        targetSdk = $null
        abis = @()
    }
    $package = [regex]::Match($text, "(?m)^package: name='([^']*)'")
    if ($package.Success) { $record.packageName = $package.Groups[1].Value }
    $versionCode = [regex]::Match($text, "versionCode='(\d+)'")
    if ($versionCode.Success) { $record.versionCode = [int]$versionCode.Groups[1].Value }
    $versionName = [regex]::Match($text, "versionName='([^']*)'")
    if ($versionName.Success) { $record.versionName = $versionName.Groups[1].Value }
    $compileSdk = [regex]::Match($text, "compileSdkVersion='(\d+)'")
    if ($compileSdk.Success) { $record.compileSdk = [int]$compileSdk.Groups[1].Value }
    $minSdk = [regex]::Match($text, "(?m)^minSdkVersion:'(\d+)'")
    if ($minSdk.Success) { $record.minSdk = [int]$minSdk.Groups[1].Value }
    $targetSdk = [regex]::Match($text, "(?m)^targetSdkVersion:'(\d+)'")
    if ($targetSdk.Success) { $record.targetSdk = [int]$targetSdk.Groups[1].Value }
    $nativeCode = [regex]::Match($text, "(?m)^native-code: '([^']*)'")
    if ($nativeCode.Success) {
        $record.abis = @($nativeCode.Groups[1].Value -split '\s+' | Where-Object { $_ })
    }
    return $record
}

function Get-GradleExpectation {
    param([string]$Root)
    $record = [ordered]@{ compileSdk = $null; minSdk = $null; targetSdk = $null }
    $path = Join-Path $Root 'apps/android/android/app/build.gradle.kts'
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return $record }
    $text = Get-Content -LiteralPath $path -Raw
    foreach ($field in @('compileSdk', 'minSdk', 'targetSdk')) {
        $match = [regex]::Match($text, "(?m)^[ \t]*$field[ \t]*=[ \t]*(\d+)")
        if ($match.Success) { $record[$field] = [int]$match.Groups[1].Value }
    }
    return $record
}

function Get-TrackedState {
    param([string]$Root, [string]$RelativePath)
    try {
        # "ls-files -- <path>" is quiet for untracked files, so recording that
        # state never fails an official build from a dirty worktree.
        $tracked = & git -C $Root ls-files -- $RelativePath
        if ($LASTEXITCODE -ne 0) { return $null }
        return [bool]$tracked
    } catch {
        return $null
    }
}

function Get-IconRecords {
    param([string]$Root)
    $densities = @('mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi')
    $relativePaths = @('assets/dvd-audio-maker.png')
    foreach ($density in $densities) {
        $relativePaths += "apps/android/android/app/src/main/res/mipmap-$density/ic_launcher.png"
        $relativePaths += "apps/android/android/app/src/main/res/drawable-$density/ic_stat_dvda.png"
    }
    $records = @()
    foreach ($relative in $relativePaths) {
        $path = Join-Path $Root $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Icon is missing (run tools/android/generate-icons.py): $path"
        }
        $records += [ordered]@{
            path = $relative
            sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
            tracked = Get-TrackedState -Root $Root -RelativePath $relative
        }
    }
    return $records
}

$apk = (Resolve-Path -LiteralPath $ApkPath).Path
if (-not (Test-Path -LiteralPath $apk -PathType Leaf)) {
    throw "APK is missing: $apk"
}
$root = (Resolve-Path -LiteralPath $RepoRoot).Path
$apkItem = Get-Item -LiteralPath $apk
$sha = (Get-FileHash -LiteralPath $apk -Algorithm SHA256).Hash.ToLowerInvariant()

$aapt2 = Get-Aapt2Path -Root $root -SdkRoot $AndroidSdkRoot
if (-not $aapt2) {
    throw "aapt2 was not found. Set -AndroidSdkRoot or ANDROID_HOME/ANDROID_SDK_ROOT."
}
$apkRecord = Read-ApkBadging -Aapt2 $aapt2 -Apk $apk
if (-not $apkRecord.versionName -or $null -eq $apkRecord.versionCode) {
    throw "aapt2 did not report versionName/versionCode for $apk"
}

$commit = Get-SourceCommit -Root $root
$version = Get-ApplicationVersion -Root $root
$gradle = Get-GradleExpectation -Root $root
$ndk = Get-NdkRecord -Root $root
$flutter = Get-FlutterRecord -Root $root
$icons = Get-IconRecords -Root $root

# Expected values come from pubspec + Gradle (falling back to the caller's
# parameters); the finished APK must agree with all of them.
$expected = [ordered]@{
    versionName = $version.versionName
    versionCode = $version.versionCode
    compileSdk = if ($null -ne $gradle.compileSdk) { $gradle.compileSdk } else { $CompileSdk }
    minSdk = if ($null -ne $gradle.minSdk) { $gradle.minSdk } else { $MinSdk }
    targetSdk = if ($null -ne $gradle.targetSdk) { $gradle.targetSdk } else { $TargetSdk }
}
$mismatches = @()
if ($expected.versionName -and $expected.versionName -ne 'unknown' -and
    $expected.versionName -ne $apkRecord.versionName) {
    $mismatches += "versionName apk=$($apkRecord.versionName) expected=$($expected.versionName)"
}
if ($null -ne $expected.versionCode -and $expected.versionCode -ne $apkRecord.versionCode) {
    $mismatches += "versionCode apk=$($apkRecord.versionCode) expected=$($expected.versionCode)"
}
foreach ($field in @('compileSdk', 'minSdk', 'targetSdk')) {
    if ($null -ne $expected[$field] -and $expected[$field] -ne $apkRecord[$field]) {
        $mismatches += "$field apk=$($apkRecord[$field]) expected=$($expected[$field])"
    }
}
$expectedAbi = if ($RustTarget -like 'aarch64*') {
    'arm64-v8a'
} elseif ($RustTarget -like 'x86_64*') {
    'x86_64'
} else {
    $null
}
if ($expectedAbi -and -not ($apkRecord.abis -contains $expectedAbi)) {
    $mismatches += "native ABI $expectedAbi is missing from the APK (got: $($apkRecord.abis -join ','))"
}
if ($mismatches.Count -gt 0) {
    throw "APK does not match the expected build inputs: $($mismatches -join '; ')"
}

$recordPath = if ($OutputPath) {
    [System.IO.Path]::GetFullPath($OutputPath)
} else {
    Join-Path $apkItem.DirectoryName 'release-build.json'
}
$directory = [System.IO.Path]::GetDirectoryName($recordPath)
if ($directory -and -not (Test-Path -LiteralPath $directory)) {
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
}

$relative = $apk
if ($apk.StartsWith($root + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
    $relative = $apk.Substring($root.Length + 1).Replace('\', '/')
}

$record = [ordered]@{
    schema = 1
    artifact = 'android-apk'
    apk = $relative
    apkBytes = $apkItem.Length
    sha256 = $sha
    packageName = $apkRecord.packageName
    abis = $apkRecord.abis
    builtAtUtc = $apkItem.LastWriteTimeUtc.ToString('yyyy-MM-ddTHH:mm:ssZ')
    recordedAtUtc = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
    commit = $commit.commit
    worktreeDirty = $commit.worktreeDirty
    versionName = $apkRecord.versionName
    versionCode = $apkRecord.versionCode
    androidTarget = $AndroidTarget
    rustTarget = $RustTarget
    ndk = $ndk
    sdk = [ordered]@{
        compileSdk = $apkRecord.compileSdk
        minSdk = $apkRecord.minSdk
        targetSdk = $apkRecord.targetSdk
    }
    expectations = $expected
    verifiedAgainstApk = $true
    aapt2 = $aapt2
    icons = [ordered]@{
        source = $icons[0]
        derived = @($icons | Select-Object -Skip 1)
    }
    flutter = $flutter
    verification = 'build-only: the APK was compiled, inspected and hashed; no device run is claimed'
    deviceVerified = $false
}

$json = $record | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($recordPath, $json + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))

Write-Output "SHA256=$sha"
Write-Output "ReleaseRecord=$recordPath"
Write-Output "Commit=$($record.commit) Version=$($record.versionName)+$($record.versionCode) Target=$AndroidTarget SDK=$($record.sdk.compileSdk)/$($record.sdk.minSdk)/$($record.sdk.targetSdk) Icons=$(@($record.icons.derived).Count)"
