param(
    [Parameter(Mandatory=$true)][string]$RustProbe,
    [Parameter(Mandatory=$true)][string]$LegacyProbe,
    [string]$Fixtures = 'build\formats-real-parity',
    [string]$Output = 'build\bridge-decoder-parity'
)
# Windows PowerShell treats redirected native stderr as error records.
$ErrorActionPreference = 'Continue'
$RustProbe = (Resolve-Path $RustProbe).Path
$LegacyProbe = (Resolve-Path $LegacyProbe).Path
New-Item -ItemType Directory -Force $Output | Out-Null
$Output = (Resolve-Path $Output).Path
$inputs = @(Get-ChildItem $Fixtures -Filter '*.mlp' | Where-Object { $_.Name -notlike '*.aligned.mlp' })
if ($inputs.Count -eq 0) { throw 'No MLP fixtures found' }
$checked = 0
foreach ($inputFile in $inputs) {
    $rustPcm = Join-Path $Output 'rust.pcm'
    $legacyPcm = Join-Path $Output 'legacy.pcm'
    $rustJson = & $RustProbe $inputFile.FullName $rustPcm 2> (Join-Path $Output 'rust.stderr')
    $rustStatus = $LASTEXITCODE
    $legacyJson = & $LegacyProbe $inputFile.FullName $legacyPcm 2> (Join-Path $Output 'legacy.stderr')
    $legacyStatus = $LASTEXITCODE
    if ($rustStatus -ne 0 -or $legacyStatus -ne 0) { throw "Decode failed for $($inputFile.Name): Rust=$rustStatus C=$legacyStatus" }
    if ($rustJson -ne $legacyJson) { throw "Metadata differs for $($inputFile.Name)" }
    if ((Get-FileHash $rustPcm).Hash -ne (Get-FileHash $legacyPcm).Hash) { throw "PCM differs for $($inputFile.Name)" }
    $checked++
}
$invalid = Join-Path $Output 'invalid.mlp'
[IO.File]::WriteAllBytes($invalid, [byte[]](0,1,2,3,255))
foreach ($probe in @($RustProbe,$LegacyProbe)) {
    & $probe $invalid (Join-Path $Output 'invalid.pcm') 1> $null 2> (Join-Path $Output 'invalid.stderr')
    if ($LASTEXITCODE -ne 1) { throw "Malformed-input status differs: $probe" }
    & $probe $inputs[0].FullName $Output 1> $null 2> (Join-Path $Output 'output.stderr')
    if ($LASTEXITCODE -ne 1) { throw "Output-error status differs: $probe" }
}
$rustCaps = & $RustProbe --capabilities 2> (Join-Path $Output 'rust-capabilities.stderr')
$rustStatus = $LASTEXITCODE
$legacyCaps = & $LegacyProbe --capabilities 2> (Join-Path $Output 'legacy-capabilities.stderr')
$legacyStatus = $LASTEXITCODE
if ($rustStatus -ne $legacyStatus -or $rustCaps -ne $legacyCaps) { throw 'Capability status/output differs' }
Remove-Item $rustPcm,$legacyPcm,$invalid,(Join-Path $Output 'invalid.pcm') -ErrorAction SilentlyContinue
Write-Output "PASS: $checked PCM/metadata fixtures, malformed input, output failure, capabilities (status $rustStatus)"
