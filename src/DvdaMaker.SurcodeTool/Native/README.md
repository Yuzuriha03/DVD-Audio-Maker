# MLP core

Frozen on 2026-10-01. `win-x86/mlp_encode.exe` is embedded by the managed project; it does not launch or load the original SurCode executable. SHA256: `88d52e9d1726a54a44bc23d7062a906a8e9ce573726510455b9a6130fa7b3afd`.

`source/` contains the 40 source/header/include dependencies of this standalone encoder, copied byte-for-byte from the source snapshot. File hashes and the upstream validation scope are in `mlpencoder-validation.json`. The core only imports Windows KERNEL32 and MSVCRT.

Use `build.cmd` with Windows MinGW-w64 i686 GCC on PATH to produce `win-x86/mlp_encode.rebuilt.exe`. Set `MLP_CC` to the compiler path if needed. Rebuilds do not replace the pinned validated binary. Compiler changes can affect encoding decisions and must pass original whole-file comparisons before updating the pinned resource and its C# SHA256 constant.

Keep the x86 floating-point implementation; the x64 .NET host invokes it as a child process to preserve validated x87 precision. No original program or vendor DLL is required. See [application behavior and metadata policy](../../../docs/MLP-ENCODER.md).
