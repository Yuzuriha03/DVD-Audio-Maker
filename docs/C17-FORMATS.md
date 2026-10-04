# C17 format runtime

`tools/formats-native/dvda-formats.c` is the narrow native boundary for deterministic format work. It contains:

- streaming MLP access-unit inspection, CRC/parity checks and alignment repair;
- byte-for-byte PCM comparison with an explicit zero-tail allowance;
- PTS, sample-rate, peak-rate and MLP checksum parsing.

The DLL does not contain the MLP encoder. Encoding remains in the existing encoder core. The C# classes keep their managed implementations as a development fallback, while Windows x64 release builds load `dvda-formats.dll` from the bundled `menu-bin` runtime.

## Build

```bat
python tools\win-build\build-formats-runtime.py --msys-root "C:\msys64" --output build\formats-native
```

The build requires the x64 MSYS2 MinGW GCC toolchain. The resulting DLL imports only Windows system libraries (`kernel32.dll` and the system C runtime); no third-party format library is copied into the package. `formats-build.json` records the source and output hashes for packaging validation.

## Validation

The normal compatibility runner remains 115 tests. Set `DVDA_FORMATS_NATIVE_DIR` to run those tests through the DLL wherever a file-based helper is used:

```bat
set "DVDA_FORMATS_NATIVE_DIR=%CD%\build\formats-native"
tests\DvdaMaker.CompatibilityTests\bin\Debug\net10.0-windows\win-x64\DvdaMaker.CompatibilityTests.exe
```

For a byte-level managed/native comparison, use the additional sample command. It temporarily selects the managed fallback, computes the alignment result, then selects the C17 DLL and compares every output byte and every reported change:

```bat
tests\DvdaMaker.CompatibilityTests\bin\Debug\net10.0-windows\win-x64\DvdaMaker.CompatibilityTests.exe ^
  --native-format-samples "D:\samples\mlp"
```

The sample command also compares inspection fields for every file and checks the PTS vectors. It is separate from the numbered 115-test count so the established compatibility baseline remains comparable.
