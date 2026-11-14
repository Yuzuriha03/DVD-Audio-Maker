$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio/Installer/vswhere.exe"
$installation = & $vswhere -latest -products "*" -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $installation) { throw "Visual Studio C++ x64 build tools are required for the host ABI probe." }
$devcmd = Join-Path $installation "Common7/Tools/VsDevCmd.bat"
$environment = & $env:ComSpec /d /c "call `"$devcmd`" -no_logo -arch=x64 -host_arch=x64 >nul && set"
if ($LASTEXITCODE -ne 0) { throw "Cannot initialize the MSVC host environment." }
foreach ($line in $environment) {
    if ($line -match "^([^=]+)=(.*)$") {
        [Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
    }
}
$localNdk = Join-Path $repo "build/android-ndk-r30"
if (Test-Path -LiteralPath $localNdk) {
    $env:ANDROID_NDK_ROOT = $localNdk
}
if (-not $env:ANDROID_NDK_ROOT) {
    throw "Android NDK not found. Install it to build/android-ndk-r30 or set ANDROID_NDK_ROOT."
}
$ndk = (Resolve-Path -LiteralPath $env:ANDROID_NDK_ROOT).Path
$bin = Join-Path $ndk "toolchains/llvm/prebuilt/windows-x86_64/bin"
$clang = Join-Path $bin "aarch64-linux-android24-clang.cmd"
$ar = Join-Path $bin "llvm-ar.exe"
foreach ($path in @($clang, $ar, (Join-Path $ndk "build/cmake/android.toolchain.cmake"))) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Invalid Android NDK installation: missing $path"
    }
}
$env:ANDROID_NDK_ROOT = $ndk
$env:ANDROID_NDK_HOME = $ndk
$env:ANDROID_NDK = $ndk
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $clang
$env:CC_aarch64_linux_android = $clang
$env:AR_aarch64_linux_android = $ar
$env:DVDA_HOST_CC = "cl.exe"
$env:DVDA_FFMPEG_PREFIX = Join-Path $repo "deps/android-aarch64"
$env:Path = "$bin;$env:Path"
Write-Output "ANDROID_NDK_ROOT=$ndk"
Write-Output "Android target=aarch64-linux-android API=24"
Write-Output "DVDA_FFMPEG_PREFIX=$env:DVDA_FFMPEG_PREFIX"
