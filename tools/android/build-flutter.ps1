$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$env:JAVA_HOME = Join-Path $root "build/jdk17"
$env:ANDROID_HOME = Join-Path $root "build/android-sdk"
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
[Environment]::SetEnvironmentVariable("DVDA_ANDROID_NDK", (Join-Path $root "build/android-ndk-r30"), "Process")
$env:PATH = "$env:JAVA_HOME/bin;$env:ANDROID_HOME/platform-tools;$env:PATH"
$env:GIT_CONFIG_COUNT = "1"
$env:GIT_CONFIG_KEY_0 = "safe.directory"
$env:GIT_CONFIG_VALUE_0 = (Resolve-Path "$root/build/flutter").Path.Replace("\", "/")

if (-not $env:TEMP) { throw "TEMP is not set." }
$tempBase = Join-Path $env:TEMP "dvda-android-build"
$env:CARGO_TARGET_DIR = Join-Path $tempBase "cargo-target"
$gradleHome = Join-Path $tempBase "gradle-home"
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
    & robocopy.exe $app $stagedApp /E /XD build .dart_tool .gradle /R:1 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null
    $stageExit = $LASTEXITCODE
    if ($stageExit -ge 8) { throw "Cannot stage Flutter project in TEMP (robocopy exit $stageExit)." }

    $gradleZip = Join-Path $root "build/gradle-9.5.0-all.zip"
    if (Test-Path -LiteralPath $gradleZip) {
        $wrapperProperties = Join-Path $stagedApp "android/gradle/wrapper/gradle-wrapper.properties"
        $distributionUrl = ([System.Uri]$gradleZip).AbsoluteUri
        $wrapperText = [System.IO.File]::ReadAllText($wrapperProperties)
        $wrapperText = $wrapperText -replace "distributionUrl=.*", "distributionUrl=$distributionUrl"
        [System.IO.File]::WriteAllText($wrapperProperties, $wrapperText)
    }

    $native = Join-Path $stagedApp "android/app/src/main/jniLibs/arm64-v8a"
    New-Item -ItemType Directory -Path $native -Force | Out-Null
    Copy-Item -LiteralPath "$env:CARGO_TARGET_DIR/aarch64-linux-android/release/libdvda_android.so" -Destination "$native/libdvda_android.so" -Force

    Copy-Item -LiteralPath (Join-Path $root "tools/android/gradle-mirrors.init.gradle.kts") -Destination (Join-Path $gradleHome "init.gradle.kts") -Force
    $sdkInitDir = Join-Path $gradleHome "init.d"
    New-Item -ItemType Directory -Path $sdkInitDir -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $root "tools/android/android-sdk.init.gradle") -Destination (Join-Path $sdkInitDir "android-sdk.gradle") -Force
    Write-Output "Android compileSdk=37 buildTools=37.0.0 minSdk=24 targetSdk=37"
    $env:GRADLE_USER_HOME = $gradleHome
    $flutter = Join-Path $root "build/flutter/bin/flutter.bat"
    Push-Location $stagedApp
    try {
        & $flutter build apk --release --target-platform android-arm64
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } finally {
        Pop-Location
    }

    $builtApk = Join-Path $stagedApp "build/app/outputs/flutter-apk/app-release.apk"
    if (-not (Test-Path -LiteralPath $builtApk)) { throw "Gradle succeeded but APK is missing: $builtApk" }
    $output = Join-Path $root "apps/android/build/app/outputs/flutter-apk"
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    Copy-Item -LiteralPath $builtApk -Destination (Join-Path $output "app-release.apk") -Force
    Write-Output "APK=$(Join-Path $output "app-release.apk")"
} finally {
    $resolvedTempBase = [System.IO.Path]::GetFullPath($tempBase)
    $resolvedWorkRun = [System.IO.Path]::GetFullPath($workRun)
    if ($resolvedWorkRun.StartsWith($resolvedTempBase + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Test-Path -LiteralPath $resolvedWorkRun)) {
        Remove-Item -LiteralPath $resolvedWorkRun -Recurse -Force
    }
}
