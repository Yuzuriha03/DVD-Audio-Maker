param(
    [string]$MediaRuntime = "build\rust-author-production",
    [string]$ImageRuntime = "build\rust-author-production",
    [string]$EncoderLibrary = "",
    [string]$AuthorRuntime = "build\rust-author-production",
    [string]$MenuData = "build\rust-author-production\data",
    [string]$MenuRuntime = "build\rust-author-production",
    [string]$MenuVendor = "build\menu-direct-vendor-rust-session",
    [string]$OtherVolume = ""
)
$ErrorActionPreference = "Stop"
$env:DVDA_MEDIA_NATIVE_DIR = (Resolve-Path -LiteralPath $MediaRuntime).Path
if ($EncoderLibrary) {
    $env:DVDA_ENCODER_LIBRARY = (Resolve-Path -LiteralPath $EncoderLibrary).Path
}
$env:DVDA_IMAGE_NATIVE_DIR = (Resolve-Path -LiteralPath $ImageRuntime).Path
$env:DVDA_TEST_AUTHOR = (Resolve-Path -LiteralPath (Join-Path $AuthorRuntime "dvda-author-dev.exe")).Path
$env:DVDA_TEST_MENU_DATA = (Resolve-Path -LiteralPath $MenuData).Path
$menuRuntimePath = (Resolve-Path -LiteralPath $MenuRuntime).Path
$menuVendorPath = (Resolve-Path -LiteralPath $MenuVendor).Path
$env:MAGICK_CONFIGURE_PATH = $env:DVDA_IMAGE_NATIVE_DIR
$env:MAGICK_FONT_PATH = $env:DVDA_IMAGE_NATIVE_DIR
$env:PATH = $env:DVDA_MEDIA_NATIVE_DIR + ';' + $env:PATH
if ($OtherVolume) {
    $env:DVDA_TEST_OTHER_VOLUME = (Resolve-Path -LiteralPath $OtherVolume).Path
}
# All input music, profiles and output discs are generated in isolated temp dirs.
$testArguments = @("--include-ignored")
if (-not $env:DVDA_TEST_OTHER_VOLUME) {
    Write-Host "Cross-volume publication case requires -OtherVolume <directory> on a second volume."
    $testArguments += @("--skip", "encoded_publication_supports_configured_staging_on_another_volume")
}
# Compile with static archives, then run the resulting binaries with runtime
# paths. Cargo must not interpret a runtime DLL directory as a vendor archive.
$env:DVDA_MENU_NATIVE_DIR = $menuVendorPath
$compiled = @(& cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --features dvda-core/direct-bridges,dvda-native/direct-bridges --offline --no-run --message-format=json)
if ($LASTEXITCODE -ne 0) { throw 'Workspace test compilation failed' }
$executables = @($compiled | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable } |
    ForEach-Object { $_.executable } | Select-Object -Unique)
if ($executables.Count -eq 0) { throw 'Cargo produced no test executables' }
$env:DVDA_MENU_NATIVE_DIR = $menuRuntimePath
foreach ($executable in $executables) {
    & $executable @testArguments
    if ($LASTEXITCODE -ne 0) { throw "Workspace tests failed: $executable" }
}
exit 0
