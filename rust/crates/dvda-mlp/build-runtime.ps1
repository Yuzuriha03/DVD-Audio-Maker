param(
    [string]$OutputDirectory = 'build\mlp-encoder',
    [string]$FrozenEncoder = 'build\remaining-migration-oracle\encoder.dll',
    [string]$TimingReference = 'build\mlp-timing-reference.dll',
    [string]$GroupReference = 'build\mlp-group-reference.dll',
    [string]$EntropyReference = 'build\mlp-entropy-reference.dll',
    [string]$QueueReference = 'build\mlp-queue-reference.dll',
    [string]$PrepareReference = 'build\mlp-prepare-reference.dll',
    [string]$PlanningReference = 'build\mlp-planning-reference.dll',
    [string]$Gcc = 'C:\msys64\mingw64\bin\gcc.exe'
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
Push-Location $root
try {
    $frozen = (Resolve-Path $FrozenEncoder).Path
    $frozenHash = (Get-FileHash $frozen -Algorithm SHA256).Hash
    if ($frozenHash -ne 'ECE6D0A8033A26E2528042A7B74C66C249EA3C8D7378C06809FB94C8F6BD79B8') { throw 'Unexpected frozen encoder oracle' }
    New-Item -ItemType Directory -Path build -Force | Out-Null
    & $Gcc -shared -O2 -std=c99 -ffp-contract=off -I native\mlp-encoder\source rust\crates\dvda-mlp\tests\group_reference.c native\mlp-encoder\source\mlp_format.c -o $GroupReference -lm
    if ($LASTEXITCODE -ne 0) { throw 'Group reference build failed' }
    & $Gcc -shared -O2 -std=c99 -I native\mlp-encoder\source native\mlp-encoder\source\mlp_output_timing.c -o $TimingReference
    if ($LASTEXITCODE -ne 0) { throw 'Timing reference build failed' }
    & $Gcc -shared -O2 -std=c99 -I native\mlp-encoder\source rust\crates\dvda-mlp\tests\entropy_reference.c native\mlp-encoder\source\mlp_cost.c native\mlp-encoder\source\mlp_select.c -o $EntropyReference
    if ($LASTEXITCODE -ne 0) { throw 'Entropy reference build failed' }
    & $Gcc -shared -O2 -std=c99 -mfpmath=387 -fexcess-precision=standard -ffp-contract=off -I native\mlp-encoder\source rust\crates\dvda-mlp\tests\planning_reference.c native\mlp-encoder\source\mlp_interval.c native\mlp-encoder\source\mlp_scale.c native\mlp-encoder\source\mlp_predict.c native\mlp-encoder\source\mlp_search.c native\mlp-encoder\source\mlp_parameters.c native\mlp-encoder\source\mlp_bits.c native\mlp-encoder\source\mlp_matrix.c native\mlp-encoder\source\mlp_restart.c native\mlp-encoder\source\mlp_substream.c native\mlp-encoder\source\mlp_entropy.c -o $PlanningReference -lm
    if ($LASTEXITCODE -ne 0) { throw 'Planning reference build failed' }
    & $Gcc -shared -O2 -std=c99 -I native\mlp-encoder\source rust\crates\dvda-mlp\tests\queue_reference.c native\mlp-encoder\source\mlp_output_queue.c native\mlp-encoder\source\mlp_output_timing.c native\mlp-encoder\source\mlp_rate.c native\mlp-encoder\source\mlp_stamp.c -o $QueueReference
    if ($LASTEXITCODE -ne 0) { throw 'Queue/rate/stamp reference build failed' }
    $prepareSources = Get-ChildItem native\mlp-encoder\source\mlp_*.c | ForEach-Object { $_.FullName }
    & $Gcc -shared -O2 -std=c99 -mfpmath=387 -fexcess-precision=standard -ffp-contract=off -I native\mlp-encoder\source rust\crates\dvda-mlp\tests\prepare_reference.c @prepareSources -o $PrepareReference -lm
    if ($LASTEXITCODE -ne 0) { throw 'Prepare reference build failed' }
    $env:MLP_PREPARE_REFERENCE = (Resolve-Path $PrepareReference).Path
    $env:MLP_QUEUE_REFERENCE = (Resolve-Path $QueueReference).Path
    $env:MLP_PLANNING_REFERENCE = (Resolve-Path $PlanningReference).Path
    $env:MLP_ENTROPY_REFERENCE = (Resolve-Path $EntropyReference).Path
    $env:MLP_TIMING_REFERENCE = (Resolve-Path $TimingReference).Path
    $env:MLP_GROUP_REFERENCE = (Resolve-Path $GroupReference).Path
    $env:MLP_FROZEN_REFERENCE = $frozen
    & cargo test --manifest-path rust\Cargo.toml -p dvda-mlp
    if ($LASTEXITCODE -ne 0) { throw 'Encoder tests failed' }
    & cargo clippy --manifest-path rust\Cargo.toml -p dvda-mlp --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Encoder lint failed' }
    & cargo build --manifest-path rust\Cargo.toml -p dvda-mlp --release
    if ($LASTEXITCODE -ne 0) { throw 'Encoder release build failed' }
    foreach ($mode in @('--standard-only', '--groups-only')) {
        & cargo run --manifest-path rust\Cargo.toml -p dvda-mlp --example differential --release -- $frozen rust\target\release\dvda_mlp.dll $mode --stress
        if ($LASTEXITCODE -ne 0) { throw "Encoder acceptance failed: $mode" }
    }
    if ((Get-FileHash $frozen -Algorithm SHA256).Hash -ne $frozenHash) { throw 'Frozen oracle changed' }
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $destination = Join-Path $OutputDirectory 'mlp_encoder.dll'
    Copy-Item rust\target\release\dvda_mlp.dll $destination -Force
    $file = Get-Item $destination
    $manifest = [ordered]@{
        implementation = 'rust'
        production_linkage = 'rlib'
        dll_role = 'differential acceptance adapter only'
        abi_version = 1
        production_ready = $false
        build_command = 'powershell -ExecutionPolicy Bypass -File rust\crates\dvda-mlp\build-runtime.ps1'
        rustc = (& rustc --version | Out-String).Trim()
        files = @{ 'mlp_encoder.dll' = @{sha256 = (Get-FileHash $destination -Algorithm SHA256).Hash.ToLowerInvariant(); bytes = $file.Length} }
        validation = @{
            unit_tests = 35
            clippy = 'all-targets -D warnings passed'
            standard_pcm_profiles = 142
            grouped_pcm_profiles = 228
            standard_stress_frames = 70001
            grouped_stress_frames = 9602
            timing_reference_transitions = 72000
            group_reference = 'complete raw MLP bytes and SHA256 match frozen C; unchanged C interpolation adapter; original header CRC/depth/rate/assignment/AU parity and decoded reconstructed PCM'
            mixed_rate_decode = 'diagnostic header normalization only; not native mixed-rate player compatibility'
            callbacks = 'fragmented reads; immediate/late input/output failure; first-output bound; counters; nested reentrancy'
            concurrency = 'eight deterministic standard DLL encodes; eight equal-rate and half-rate grouped DLL and direct Rust encodes'
            direct_api = 'bounded slices; fragmented independent grouped reads; callback panic containment; cancellation; counters; invalid grouped duration'
            metadata = 'stuffed multi-record and partial valid-bit streams match C; malformed early admission'
            malformed_abi_cases = 13
            compression = 'entropy modes0-3; adaptive FIR1-8 and stable IIR1-4; multichannel reversible matrix; lossless common-bit scaling'
            frozen_encoder_sha256 = $frozenHash.ToLowerInvariant()
        }
        remaining_acceptance = @('Allocation exhaustion still aborts for infallible Rust allocations rather than returning ABI -2', 'Exhaustive arbitrary-input/FIFO capability comparison not yet certified', 'Native mixed-rate player compatibility unverified; current independent reconstructed-PCM acceptance is documented')
    }
    $manifest | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $OutputDirectory 'encoder-build.json') -Encoding UTF8
} finally { Pop-Location }
