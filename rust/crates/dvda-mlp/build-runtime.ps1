param(
    [string]$OutputDirectory = 'build\mlp-encoder'
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
Push-Location $root
try {
    & cargo test --manifest-path rust\Cargo.toml -p dvda-mlp --offline
    if ($LASTEXITCODE -ne 0) { throw 'Encoder tests failed' }
    & cargo clippy --manifest-path rust\Cargo.toml -p dvda-mlp --all-targets --offline -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Encoder lint failed' }
    $buildMessages = & cargo build --manifest-path rust\Cargo.toml -p dvda-mlp --release --offline --message-format=json
    if ($LASTEXITCODE -ne 0) { throw 'Encoder release build failed' }
    $artifacts = @($buildMessages | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object {
        $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'dvda_mlp'
    })
    $libraries = @($artifacts | ForEach-Object { $_.filenames } | Where-Object { $_ -like '*.dll' })
    if ($libraries.Count -ne 1) { throw 'Expected exactly one dvda-mlp DLL build artifact' }
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $destination = Join-Path $OutputDirectory 'mlp_encoder.dll'
    Copy-Item -LiteralPath $libraries[0] -Destination $destination -Force
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
        }
    }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'encoder-build.json') -Encoding UTF8
} finally {
    Pop-Location
}
