param(
    [string]$OutputDirectory = 'build\mlp-encoder',
    [string]$FrozenEncoder = ''
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$previousReference = $env:MLP_FROZEN_REFERENCE
Push-Location $root
try {
    $frozen = $null
    $frozenHash = $null
    if ($FrozenEncoder) {
        $frozen = (Resolve-Path -LiteralPath $FrozenEncoder).Path
        $frozenHash = (Get-FileHash -LiteralPath $frozen -Algorithm SHA256).Hash
        if ($frozenHash -ne 'ECE6D0A8033A26E2528042A7B74C66C249EA3C8D7378C06809FB94C8F6BD79B8') {
            throw 'Unexpected external frozen encoder oracle'
        }
        $env:MLP_FROZEN_REFERENCE = $frozen
    } else {
        Remove-Item Env:MLP_FROZEN_REFERENCE -ErrorAction SilentlyContinue
    }
    $testFeatures = @()
    if ($frozen) { $testFeatures = @('--features', 'external-oracle') }
    & cargo test --manifest-path rust\Cargo.toml -p dvda-mlp --offline @testFeatures
    if ($LASTEXITCODE -ne 0) { throw 'Encoder tests failed' }
    & cargo clippy --manifest-path rust\Cargo.toml -p dvda-mlp --all-targets --offline -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Encoder lint failed' }
    & cargo build --manifest-path rust\Cargo.toml -p dvda-mlp --release --offline
    if ($LASTEXITCODE -ne 0) { throw 'Encoder release build failed' }
    if ($frozen) {
        foreach ($mode in @('--standard-only', '--groups-only')) {
            & cargo run --manifest-path rust\Cargo.toml -p dvda-mlp --example differential --release --offline -- $frozen rust\target\release\dvda_mlp.dll $mode --stress
            if ($LASTEXITCODE -ne 0) { throw "Encoder external-oracle acceptance failed: $mode" }
        }
        if ((Get-FileHash -LiteralPath $frozen -Algorithm SHA256).Hash -ne $frozenHash) { throw 'External frozen oracle changed' }
    }
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $destination = Join-Path $OutputDirectory 'mlp_encoder.dll'
    Copy-Item -LiteralPath rust\target\release\dvda_mlp.dll -Destination $destination -Force
    $file = Get-Item -LiteralPath $destination
    $manifest = [ordered]@{
        implementation = 'rust'
        production_linkage = 'rlib'
        dll_role = 'developer ABI acceptance adapter'
        abi_version = 1
        production_ready = $false
        build_command = 'powershell -ExecutionPolicy Bypass -File rust\crates\dvda-mlp\build-runtime.ps1'
        rustc = (& rustc --version | Out-String).Trim()
        files = @{ 'mlp_encoder.dll' = @{sha256 = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant(); bytes = $file.Length} }
        validation = @{
            unit_tests = 'cargo test --offline passed'
            clippy = 'all-targets -D warnings passed'
            external_frozen_oracle_checked = [bool]$frozen
            frozen_encoder_sha256 = if ($frozenHash) { $frozenHash.ToLowerInvariant() } else { $null }
        }
    }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'encoder-build.json') -Encoding UTF8
} finally {
    if ($null -eq $previousReference) { Remove-Item Env:MLP_FROZEN_REFERENCE -ErrorAction SilentlyContinue }
    else { $env:MLP_FROZEN_REFERENCE = $previousReference }
    Pop-Location
}
