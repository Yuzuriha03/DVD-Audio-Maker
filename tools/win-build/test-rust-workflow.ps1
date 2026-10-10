param(
    [string]$MediaRuntime = "build\media-native-shared",
    [string]$ImageRuntime = "build\image-native",
    [string]$EncoderLibrary = "native\mlp-encoder\win-x64\mlp_encoder.dll",
    [string]$AuthorRuntime = "build\rust-author-current",
    [string]$MenuData = "build\release-menu-final\DVD-Audio-Maker\data",
    [string]$MenuRuntime = "build\menu-rust-production",
    [string]$OtherVolume = ""
)
$ErrorActionPreference = "Stop"
$env:DVDA_MEDIA_NATIVE_DIR = (Resolve-Path -LiteralPath $MediaRuntime).Path
$env:DVDA_ENCODER_LIBRARY = (Resolve-Path -LiteralPath $EncoderLibrary).Path
$env:DVDA_IMAGE_NATIVE_DIR = (Resolve-Path -LiteralPath $ImageRuntime).Path
$env:DVDA_TEST_AUTHOR = (Resolve-Path -LiteralPath (Join-Path $AuthorRuntime "dvda-author-dev.exe")).Path
$env:DVDA_TEST_MENU_DATA = (Resolve-Path -LiteralPath $MenuData).Path
$env:DVDA_MENU_NATIVE_DIR = (Resolve-Path -LiteralPath $MenuRuntime).Path
if ($OtherVolume) {
    $env:DVDA_TEST_OTHER_VOLUME = (Resolve-Path -LiteralPath $OtherVolume).Path
}
# Frozen metadata references intentionally require their original media build.
# All input music, profiles and output discs are generated in isolated temp dirs.
$testArguments = @("--include-ignored")
if (-not $env:DVDA_TEST_OTHER_VOLUME) {
    Write-Host "Cross-volume publication case requires -OtherVolume <directory> on a second volume."
    $testArguments += @("--skip", "encoded_publication_supports_configured_staging_on_another_volume")
}
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- @testArguments
exit $LASTEXITCODE
