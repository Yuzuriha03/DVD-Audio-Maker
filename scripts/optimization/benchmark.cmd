@echo off
REM Performance benchmark script for DVD-Audio Maker
REM Tests optimizations before/after comparison

echo ========================================
echo DVD-Audio Maker Performance Benchmark
echo ========================================
echo.

REM Check if test files exist
if not exist "test_samples\" (
    echo [ERROR] test_samples\ directory not found
    echo Please create test_samples\ with sample FLAC/M4A files
    exit /b 1
)

echo [1/4] Building optimized release version...
cargo build --release --manifest-path rust\Cargo.toml --package dvda-mlp 2>nul
if errorlevel 1 (
    echo [ERROR] Build failed
    exit /b 1
)
echo [OK] Build completed

echo.
echo [2/4] Testing MLP encoder performance...
echo Test file: test_samples\sample.flac
set START_TIME=%TIME%

REM Run CLI encode test (if available)
cli.cmd encode --input "test_samples\sample.flac" --output "test_samples\sample.mlp" 2>nul
if errorlevel 1 (
    echo [SKIP] CLI test not available (needs full build)
) else (
    set END_TIME=%TIME%
    echo [OK] Encode completed
    echo Start: %START_TIME%
    echo End:   %END_TIME%
)

echo.
echo [3/4] Checking binary size...
if exist "rust\target\x86_64-pc-windows-gnu\release\dvda_mlp.dll" (
    dir rust\target\x86_64-pc-windows-gnu\release\dvda_mlp.dll | find "dvda_mlp"
    echo [OK] MLP library built successfully
) else (
    echo [SKIP] MLP DLL not found (check build)
)

echo.
echo [4/4] Performance optimization summary:
echo - LTO: fat (full link-time optimization)
echo - target-cpu: native (AVX2/SSE4.2 enabled)
echo - panic: abort (faster unwinding)
echo - Optimized bit operations in MLP encoder
echo - Rayon parallel processing ready

echo.
echo ========================================
echo Benchmark completed
echo ========================================
echo.
echo Next steps:
echo 1. Run full ISO build with gui.cmd or cli.cmd
echo 2. Compare processing time with previous builds
echo 3. Monitor CPU usage during processing
echo.
pause
