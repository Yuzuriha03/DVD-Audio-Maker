$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$gradleVersion = "9.5.0"
$env:JAVA_HOME = Join-Path $root "build/jdk17"
$env:ANDROID_HOME = Join-Path $root "build/android-sdk"
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
$env:PUB_HOSTED_URL = "https://pub.flutter-io.cn"
$env:GRADLE_OPTS = "-Dorg.gradle.daemon=false"
[Environment]::SetEnvironmentVariable("DVDA_ANDROID_NDK", (Join-Path $root "build/android-ndk-r30"), "Process")
$localNdk = Join-Path $root "build/android-ndk-r30"
$ndkRevision = (Get-Content -LiteralPath "$localNdk/source.properties" | Where-Object { $_ -match '^Pkg\.Revision\s*=' } | Select-Object -First 1) -replace '^Pkg\.Revision\s*=\s*', ''
$sideBySideNdk = Join-Path $root "build/android-sdk/ndk/$ndkRevision"
if (-not (Test-Path -LiteralPath "$sideBySideNdk/source.properties" -PathType Leaf)) {
    # AGP consults the SDK side-by-side directory once ndkVersion is set.
    # Reuse the existing NDK locally; never invoke SDK Manager or download it.
    New-Item -ItemType Directory -Path (Split-Path -Parent $sideBySideNdk) -Force | Out-Null
    & robocopy.exe $localNdk $sideBySideNdk /E /R:1 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null
    if ($LASTEXITCODE -ge 8) { throw "Cannot stage the existing NDK locally (robocopy exit $LASTEXITCODE)." }
}
$env:PATH = "$env:JAVA_HOME/bin;$env:ANDROID_HOME/platform-tools;$env:PATH"
$env:GIT_CONFIG_COUNT = "1"
$env:GIT_CONFIG_KEY_0 = "safe.directory"
$env:GIT_CONFIG_VALUE_0 = (Resolve-Path "$root/build/flutter").Path.Replace("\", "/")

if (-not $env:TEMP) { throw "TEMP is not set." }
$tempBase = Join-Path $env:TEMP "dvda-android-build"
# Compilation and dependency caches are build inputs, not disposable TEMP
# residue. TEMP is only used for the per-run staged project.
$env:CARGO_TARGET_DIR = Join-Path $root "build/android-cargo-target"
$cachedGradleHome = Join-Path $root "build/gradle-home"
$cachedGradle = Join-Path $cachedGradleHome "wrapper/dists/gradle-$gradleVersion-all"
$gradleHome = if (Test-Path -LiteralPath $cachedGradle -PathType Container) {
    $cachedGradleHome
} else {
    Join-Path $tempBase "gradle-home"
}
$workRun = Join-Path $tempBase ("work-" + [guid]::NewGuid().ToString("N"))
$stagedApp = Join-Path $workRun "app"
New-Item -ItemType Directory -Path $env:CARGO_TARGET_DIR, $gradleHome, $stagedApp -Force | Out-Null
Write-Output "Build temp=$workRun"
Write-Output "Cargo target=$env:CARGO_TARGET_DIR"
Write-Output "Gradle home=$gradleHome"

try {
    . (Join-Path $root "tools/android/ndk-env.ps1")
    $crate = Join-Path $root "rust/crates/dvda-android"
    cargo build --release --manifest-path "$crate/Cargo.toml" --target aarch64-linux-android
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $app = Join-Path $root "apps/android"
    # Both icon families are generated from the single master PNG by
    # tools/android/generate-icons.py and committed; refuse to build with a
    # missing density instead of failing later inside resource linking.
    $iconDensities = @('mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi')
    $icons = @()
    foreach ($density in $iconDensities) {
        $icons += Join-Path $app "android/app/src/main/res/mipmap-$density/ic_launcher.png"
        $icons += Join-Path $app "android/app/src/main/res/drawable-$density/ic_stat_dvda.png"
    }
    $icons += Join-Path $root "assets/dvd-audio-maker.png"
    foreach ($icon in $icons) {
        if (-not (Test-Path -LiteralPath $icon -PathType Leaf)) {
            throw "Icon is missing (run tools/android/generate-icons.py): $icon"
        }
    }
    & robocopy.exe $app $stagedApp /E /XD build .dart_tool .gradle /R:1 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null
    $stageExit = $LASTEXITCODE
    if ($stageExit -ge 8) { throw "Cannot stage Flutter project in TEMP (robocopy exit $stageExit)." }

    # Bundle the prepared menu font collection (the only font source on every
    # platform) into the APK assets. Android only unpacks this asset into
    # app-private storage; the shared Rust font resolution and fallbacks are
    # unchanged, and no second font source is introduced.
    $fontSha256 = "b0668861fa47b917873a1e670062f64c0ea5708b7f9db6941c4ed79fb6525131"
    $fontCandidates = @(
        (Join-Path $root "build/gui-only-author-v1.2/arm64/fonts/DvdaNotoCJK-Regular.ttc"),
        (Join-Path $root "build/gui-only-author-v1.1/arm64/fonts/DvdaNotoCJK-Regular.ttc"),
        (Join-Path $root "build/gui-only-author-v1.1/x64/fonts/DvdaNotoCJK-Regular.ttc"),
        (Join-Path $root "build/msvc-native/package-assets/fonts/DvdaNotoCJK-Regular.ttc"),
        (Join-Path $root "build/font-sources/optimized/DvdaNotoCJK-Regular.ttc")
    )
    $fontSource = $null
    foreach ($candidate in $fontCandidates) {
        if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) { continue }
        $hash = (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($hash -ne $fontSha256) {
            throw "Menu font hash mismatch for $candidate : expected $fontSha256, got $hash. Reuse the prepared DvdaNotoCJK-Regular.ttc instead of creating another font source."
        }
        if ($null -eq $fontSource) { $fontSource = $candidate }
    }
    if ($null -eq $fontSource) {
        throw "Prepared menu font DvdaNotoCJK-Regular.ttc is missing. Prepare it from the project font inputs before building the APK."
    }
    $fontTarget = Join-Path $stagedApp "android/app/src/main/assets/fonts/DvdaNotoCJK-Regular.ttc"
    New-Item -ItemType Directory -Path (Split-Path -Parent $fontTarget) -Force | Out-Null
    Copy-Item -LiteralPath $fontSource -Destination $fontTarget -Force
    $copiedFontHash = (Get-FileHash -LiteralPath $fontTarget -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($copiedFontHash -ne $fontSha256) {
        throw "Staged menu font hash mismatch: $copiedFontHash"
    }
    Write-Output "Font=$fontSource"
    Write-Output "FontSha256=$fontSha256"

    $gradleDistributionRoot = Join-Path $root "build/gradle-home/wrapper/dists/gradle-$gradleVersion-all"
    $installedGradle = Get-ChildItem -Path $gradleDistributionRoot -Directory -ErrorAction SilentlyContinue |
        ForEach-Object { Join-Path $_.FullName "gradle-$gradleVersion" } |
        Where-Object {
            (Test-Path -LiteralPath (Join-Path $_ "bin/gradle.bat") -PathType Leaf) -and
            (Test-Path -LiteralPath (Join-Path $_ "lib/gradle-launcher-$gradleVersion.jar") -PathType Leaf)
        } | Select-Object -First 1
    if ($null -ne $installedGradle) {
        # Reuse the verified unpacked distribution under its original wrapper
        # key. Do not switch to file:///, which creates a second cache key and
        # needlessly extracts thousands of small source files again.
        $installedMarker = Join-Path (Split-Path -Parent $installedGradle) "gradle-$gradleVersion-all.zip.ok"
        if (-not (Test-Path -LiteralPath $installedMarker)) {
            New-Item -ItemType File -Path $installedMarker | Out-Null
        }
        Write-Output "Gradle=$installedGradle"
    } else {
    $gradleZip = Join-Path $root "build/gradle-$gradleVersion-all.zip"
    if (-not (Test-Path -LiteralPath $gradleZip)) {
        $cachedZip = Get-ChildItem -Path $gradleDistributionRoot `
            -Filter "gradle-$gradleVersion-all.zip" -File -Recurse -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($null -ne $cachedZip) { $gradleZip = $cachedZip.FullName }
    }
    if (Test-Path -LiteralPath $gradleZip) {
        $wrapperProperties = Join-Path $stagedApp "android/gradle/wrapper/gradle-wrapper.properties"
        $distributionUrl = ([System.Uri]$gradleZip).AbsoluteUri
        $wrapperText = [System.IO.File]::ReadAllText($wrapperProperties)
        $wrapperText = $wrapperText -replace "distributionUrl=.*", "distributionUrl=$distributionUrl"
        [System.IO.File]::WriteAllText($wrapperProperties, $wrapperText)
    } else {
        # No local distribution: use the verified Huawei mirror. Aliyun's
        # /gradle/gradle-$gradleVersion-all.zip currently returns 404, so it cannot serve
        # as a real distribution mirror. Never fall back to services.gradle.org.
        $wrapperProperties = Join-Path $stagedApp "android/gradle/wrapper/gradle-wrapper.properties"
        $wrapperText = [System.IO.File]::ReadAllText($wrapperProperties)
        $wrapperText = $wrapperText -replace "distributionUrl=.*", "distributionUrl=https\://mirrors.huaweicloud.com/gradle/gradle-$gradleVersion-all.zip"
        [System.IO.File]::WriteAllText($wrapperProperties, $wrapperText)
    }
    }

    $native = Join-Path $stagedApp "android/app/src/main/jniLibs/arm64-v8a"
    New-Item -ItemType Directory -Path $native -Force | Out-Null
    Copy-Item -LiteralPath "$env:CARGO_TARGET_DIR/aarch64-linux-android/release/libdvda_android.so" -Destination "$native/libdvda_android.so" -Force

    Copy-Item -LiteralPath (Join-Path $root "tools/android/gradle-mirrors.init.gradle.kts") -Destination (Join-Path $gradleHome "init.gradle.kts") -Force
    $sdkInitDir = Join-Path $gradleHome "init.d"
    New-Item -ItemType Directory -Path $sdkInitDir -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $root "tools/android/android-sdk.init.gradle") -Destination (Join-Path $sdkInitDir "android-sdk.gradle") -Force
    $compileSdk = 37
    $minSdk = 24
    $targetSdk = 37
    Write-Output "Android compileSdk=$compileSdk buildTools=37.0.0 minSdk=$minSdk targetSdk=$targetSdk"
    $env:GRADLE_USER_HOME = $gradleHome
    $flutter = Join-Path $root "build/flutter/bin/flutter.bat"
    Push-Location $stagedApp
    try {
        # Flutter and its plugins use the versions resolved by the Flutter SDK
        # and the committed lockfile; do not replace them with unrelated latest.
        & $flutter pub get
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        & $flutter build apk --release --target-platform android-arm64
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } finally {
        Pop-Location
    }

    $builtApk = Join-Path $stagedApp "build/app/outputs/flutter-apk/app-release.apk"
    if (-not (Test-Path -LiteralPath $builtApk)) { throw "Gradle succeeded but APK is missing: $builtApk" }
    $output = Join-Path $root "apps/android/build/app/outputs/flutter-apk"
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $apk = Join-Path $output "app-release.apk"
    Copy-Item -LiteralPath $builtApk -Destination $apk -Force
    Write-Output "APK=$apk"
    # Provenance record: commit, version/targets/SDK read back from the APK with
    # aapt2, NDK revision, icon hashes and the APK SHA-256. The record states
    # build-only verification; it never claims a device run.
    & (Join-Path $root "tools/android/release-metadata.ps1") `
        -ApkPath $apk `
        -RepoRoot $root `
        -AndroidTarget "android-arm64" `
        -RustTarget "aarch64-linux-android" `
        -CompileSdk $compileSdk `
        -MinSdk $minSdk `
        -TargetSdk $targetSdk `
        -AndroidSdkRoot $env:ANDROID_HOME
    Write-Output "ReleaseRecord=$(Join-Path $output "release-build.json")"
} finally {
    $resolvedTempBase = [System.IO.Path]::GetFullPath($tempBase)
    $resolvedWorkRun = [System.IO.Path]::GetFullPath($workRun)
    if ($resolvedWorkRun.StartsWith($resolvedTempBase + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Test-Path -LiteralPath $resolvedWorkRun)) {
        Remove-Item -LiteralPath $resolvedWorkRun -Recurse -Force
    }
}
