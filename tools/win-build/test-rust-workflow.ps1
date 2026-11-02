param(
    [ValidateSet('Functional', 'Benchmarks')][string]$Mode = 'Functional',
    [string]$MediaRuntime = 'build\rust-author-production',
    [string]$ImageRuntime = '',
    [string]$EncoderLibrary = '',
    [string]$MenuData = 'build\rust-author-production\data',
    [string]$MenuRuntime = 'build\rust-author-production',
    [string]$OtherVolume = ''
)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..\..')
$wrapper = Join-Path $PSScriptRoot 'cargo-msvc.ps1'
$benchmarks = @('normalization_throughput', 'packing_32_throughput', 'release_files_benchmark',
    'pcm_comparison_throughput', 'inspection_progress_callback_benchmark',
    'synthetic_batch_worker_benchmark', 'real_cached_mlp_preflight_benchmark_is_read_only')
$skipped = [System.Collections.Generic.List[string]]::new()
function Skip-Test([string]$Name, [string]$Reason) {
    $skipped.Add($Name)
    Write-Host "NOT RUN: $Name -- $Reason"
}
function Require-Path([string]$Path, [string]$PathType = 'Container') {
    if (-not $Path -or -not (Test-Path -LiteralPath $Path -PathType $PathType)) { throw "Required $PathType path not found: $Path" }
    return (Resolve-Path -LiteralPath $Path).Path
}
function Require-WritableDirectory([string]$Path) {
    $directory = Require-Path $Path
    $probe = Join-Path $directory ([Guid]::NewGuid().ToString() + '.tmp')
    try { [IO.File]::WriteAllText($probe, '') } finally { if (Test-Path -LiteralPath $probe) { Remove-Item -LiteralPath $probe } }
}
function Show-CompilerMessages($Lines) {
    foreach ($line in $Lines) {
        $message = $line | ConvertFrom-Json
        if ($message.reason -eq 'compiler-message' -and $message.message.rendered) {
            Write-Host $message.message.rendered
        }
    }
}
$env:DVDA_MEDIA_NATIVE_DIR = Require-Path $MediaRuntime
Remove-Item Env:DVDA_IMAGE_NATIVE_DIR -ErrorAction SilentlyContinue
Remove-Item Env:MAGICK_CONFIGURE_PATH -ErrorAction SilentlyContinue
Remove-Item Env:MAGICK_FONT_PATH -ErrorAction SilentlyContinue
if ($ImageRuntime) {
    $env:DVDA_IMAGE_NATIVE_DIR = Require-Path $ImageRuntime
    $imageOracle = Require-Path (Join-Path $env:DVDA_IMAGE_NATIVE_DIR 'dvda-image.dll') Leaf
    if ((Get-FileHash -LiteralPath $imageOracle -Algorithm SHA256).Hash -ne 'AC8AFB08C95B980A16E0212538403058B1B71340C78159DC19D6642CACEDF1DB') {
        throw 'ImageRuntime must contain the explicitly supplied frozen image oracle.'
    }
    $env:MAGICK_CONFIGURE_PATH = $env:DVDA_IMAGE_NATIVE_DIR
    $env:MAGICK_FONT_PATH = $env:DVDA_IMAGE_NATIVE_DIR
    $env:PATH = "$env:DVDA_IMAGE_NATIVE_DIR;$env:PATH"
} else {
    Skip-Test 'pixels_diagnostics_concurrency_and_failure_protection' 'explicit frozen image oracle required via -ImageRuntime'
}
if (-not $ImageRuntime -or -not $env:DVDA_IMAGE_TEST_CMYK_ICC) {
    Skip-Test 'decoder_color_and_first_frame_policies' 'requires -ImageRuntime and an external CMYK ICC profile via DVDA_IMAGE_TEST_CMYK_ICC'
} else {
    $env:DVDA_IMAGE_TEST_CMYK_ICC = Require-Path $env:DVDA_IMAGE_TEST_CMYK_ICC Leaf
}
$env:DVDA_TEST_MENU_DATA = Require-Path $MenuData
$env:DVDA_MENU_NATIVE_DIR = Require-Path $MenuRuntime
if ($EncoderLibrary) { $env:DVDA_ENCODER_LIBRARY = Require-Path $EncoderLibrary Leaf }
if ($env:DVDA_ENCODER_LIBRARY) {
    $oracle = Require-Path $env:DVDA_ENCODER_LIBRARY Leaf
    if ((Get-FileHash -LiteralPath $oracle -Algorithm SHA256).Hash -ne 'ECE6D0A8033A26E2528042A7B74C66C249EA3C8D7378C06809FB94C8F6BD79B8') {
        throw 'EncoderLibrary must be the explicitly supplied frozen C oracle with the documented SHA256.'
    }
} else {
    $oracleTests = if ($Mode -eq 'Functional') {
        @('frozen_mlp_matrix_and_lossless_decode', 'encoder_transactions_and_parallel_calls',
          'batch_matrix_matches_frozen_encoder_bytes', 'mlp_acquisition_upgrades_legacy_parameter_cache_without_reencoding_or_track_changes',
          'batch_cancellation_panic_and_partial_failure')
    } else { @('synthetic_batch_worker_benchmark') }
    foreach ($name in $oracleTests) { Skip-Test $name 'explicit frozen encoder oracle required via -EncoderLibrary' }
}
if ($Mode -eq 'Benchmarks' -and -not $skipped.Contains('synthetic_batch_worker_benchmark')) {
    $root = $env:DVDA_BATCH_BENCH_ROOT
    if ($env:DVDA_BATCH_BENCH -ne '1' -or -not $root -or (Test-Path -LiteralPath $root) -or
        -not $env:DVDA_BATCH_BENCH_EXE -or -not (Test-Path -LiteralPath $env:DVDA_BATCH_BENCH_EXE -PathType Leaf)) {
        Skip-Test 'synthetic_batch_worker_benchmark' 'requires DVDA_BATCH_BENCH=1, a new DVDA_BATCH_BENCH_ROOT and a packaged DVDA_BATCH_BENCH_EXE'
    }
}
$env:PATH = "$env:DVDA_MEDIA_NATIVE_DIR;$env:PATH"
if ($OtherVolume) { $env:DVDA_TEST_OTHER_VOLUME = Require-Path $OtherVolume }
if ($Mode -eq 'Functional') {
    foreach ($name in $benchmarks) { Skip-Test $name 'manual benchmark; use -Mode Benchmarks' }
    $other = $env:DVDA_TEST_OTHER_VOLUME
    if (-not $other) {
        Skip-Test 'encoded_publication_supports_configured_staging_on_another_volume' 'set -OtherVolume to a writable directory on another volume'
    } else {
        $other = Require-Path $other
        if ([IO.Path]::GetPathRoot($other) -eq [IO.Path]::GetPathRoot([IO.Path]::GetTempPath())) {
            throw 'OtherVolume must be on a different volume from the system temporary directory.'
        }
        $probe = Join-Path $other ([Guid]::NewGuid().ToString() + '.tmp')
        try { [IO.File]::WriteAllText($probe, '') } finally { if (Test-Path $probe) { Remove-Item $probe } }
    }
    if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) {
        Skip-Test 'native_float_lossless_matrix' 'ffmpeg executable is not on PATH'
    }
} else {
    foreach ($prefix in @('DVDA_HASH_BENCH', 'DVDA_MLP_PREFLIGHT')) {
        $names = if ($prefix -eq 'DVDA_HASH_BENCH') { @('INPUTS', 'REPORT') } else { @('MANIFEST', 'ROOT', 'OUTPUT', 'MEDIA', 'ENCODER', 'REPORT') }
        $reason = @()
        foreach ($suffix in $names) {
            $key = "${prefix}_$suffix"
            $value = [Environment]::GetEnvironmentVariable($key)
            if (-not $value) { $reason += "$key unset"; continue }
            if ($suffix -eq 'INPUTS') {
                try {
                    if (-not $value.TrimStart().StartsWith('[')) { throw 'expected a JSON array of paths' }
                    $inputs = @($value | ConvertFrom-Json)
                    if ($inputs.Count -eq 0) { throw 'empty input list' }
                    foreach ($inputPath in $inputs) { if (-not (Test-Path -LiteralPath $inputPath -PathType Leaf)) { throw "missing input: $inputPath" } }
                } catch { $reason += "${key}: $_" }
            } elseif ($suffix -eq 'REPORT') {
                $parent = Split-Path ([IO.Path]::GetFullPath($value))
                try {
                    if (Test-Path -LiteralPath $value -PathType Container) { throw 'report path is a directory' }
                    Require-WritableDirectory $parent
                } catch { $reason += "${key}: $_" }
            } else {
                $pathType = if ($suffix -in @('ROOT', 'OUTPUT')) { 'Container' } else { 'Leaf' }
                if (-not (Test-Path -LiteralPath $value -PathType $pathType)) { $reason += "$key requires an existing $pathType path" }
            }
        }
        if ($reason.Count) {
            $name = if ($prefix -eq 'DVDA_HASH_BENCH') { 'release_files_benchmark' } else { 'real_cached_mlp_preflight_benchmark_is_read_only' }
            Skip-Test $name ($reason -join '; ')
        }
    }
}
$common = @('--manifest-path', 'rust\Cargo.toml', '--target', 'x86_64-pc-windows-msvc', '--offline')
if ($Mode -eq 'Benchmarks') { $common += '--release' }
$stage = $null
try {
    if ($Mode -eq 'Functional') {
        if ($env:DVDA_TEST_AUTHOR_QUICK) { throw 'Unset DVDA_TEST_AUTHOR_QUICK for the full functional matrix.' }
        $built = @(& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $wrapper build @common -p dvda-cli --bin dvda-author-dev --message-format=json)
        Show-CompilerMessages $built
        if ($LASTEXITCODE -ne 0) { throw 'Test author build failed' }
        $author = @($built | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'dvda-author-dev' -and $_.executable } | ForEach-Object { $_.executable })
        if ($author.Count -ne 1) { throw 'Expected one freshly built test author artifact' }
        $stage = Join-Path (Get-Location) ('build\rust-workflow\author-' + [Guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $stage | Out-Null
        Copy-Item -LiteralPath $author[0] -Destination $stage
        foreach ($runtime in @($env:DVDA_MEDIA_NATIVE_DIR)) {
            Get-ChildItem -LiteralPath $runtime -Filter '*.dll' | Copy-Item -Destination $stage -Force
        }
        $env:DVDA_TEST_AUTHOR = Join-Path $stage 'dvda-author-dev.exe'
        Write-Host "Test author: $env:DVDA_TEST_AUTHOR"
    }
    $packages = @(if ($Mode -eq 'Benchmarks') { '-p'; 'dvda-core'; '-p'; 'dvda-native' } else { '--workspace' })
    $compiled = @(& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $wrapper test @common @packages --features dvda-core/direct-bridges,dvda-native/direct-bridges --no-run --message-format=json)
    Show-CompilerMessages $compiled
    if ($LASTEXITCODE -ne 0) { throw 'Workspace test compilation failed' }
    $executables = @($compiled | ForEach-Object { $_ | ConvertFrom-Json } |
        Where-Object { $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable } |
        ForEach-Object { $_.executable } | Select-Object -Unique)
    if ($executables.Count -eq 0) { throw 'Cargo produced no test executables' }
    $failed = [System.Collections.Generic.List[string]]::new()
    foreach ($executable in $executables) {
        if ($Mode -eq 'Functional') {
            $testArguments = @('--include-ignored')
            foreach ($name in $skipped) { $testArguments += @('--skip', $name) }
            & $executable @testArguments
            if ($LASTEXITCODE -ne 0) { $failed.Add($executable) }
        } else {
            $listed = @(& $executable --list --ignored)
            if ($LASTEXITCODE -ne 0) { throw "Test discovery failed: $executable" }
            foreach ($line in $listed) {
                if ($line -match '^(.+): test$') {
                    $name = $matches[1]
                    $short = ($name -split '::')[-1]
                    if ($benchmarks -contains $short -and -not $skipped.Contains($short)) {
                        & $executable --ignored --exact $name --nocapture
                        if ($LASTEXITCODE -ne 0) { $failed.Add("${executable}: $name") }
                    }
                }
            }
        }
    }
    Write-Host "$Mode finished. Explicitly not run: $($skipped.Count). Failed executables: $($failed.Count). Other mode's tests are not included."
    if ($failed.Count) { throw "Failed test executables: $($failed -join '; ')" }
} finally {
    if ($stage -and (Test-Path -LiteralPath $stage)) { Remove-Item -LiteralPath $stage -Recurse -Force }
}
