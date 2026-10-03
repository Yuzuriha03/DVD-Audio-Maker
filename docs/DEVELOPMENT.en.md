# Development and debugging

[简体中文](DEVELOPMENT.md) | [English](DEVELOPMENT.en.md)

Standard user packages contain only the GUI entry point. CLI source, solution projects and tests remain available. The GUI calls shared workflow libraries directly and does not spawn the CLI. Development requires Windows x64 and the .NET 10 SDK.

Media operations now use in-process DLLs. Copy media-native from a validated release to build/media-native before running from source, or follow the [Windows build instructions](../tools/win-build/README.en.md). Routine C# changes do not require native recompilation. NativeMediaDirectory selects the MSBuild input; DVDA_MEDIA_NATIVE_DIR can select a runtime directory. The GUI needs no FFmpeg/FFprobe installation; comparison integration tests still need reference tools.

## Source-checkout commands

```bat
gui-debug.cmd
cli.cmd config
cli.cmd config --config "C:\work\test settings.env" --shell
cli.cmd prepare --config "C:\work\test settings.env"
cli.cmd build --dry-run --config "C:\work\test settings.env"
dotnet run --project tests/DvdaMaker.CompatibilityTests -c Debug
```

The new scripts build Debug / win-x64, forward arguments and preserve exit codes. Debug builds retain PDB symbols; release packages omit development symbols. Existing gui.cmd, build.cmd and verify.cmd remain usable. Use an isolated test configuration when reproducing issues, since preparation and encoding can modify selected source and working directories.

Packaging uses a freshly cleaned tools/win-build/publish/build-artifacts-win-x64 compilation directory. It does not reuse development bin/obj outputs, preventing stale debug records from entering release assemblies.

## VS Code

With the recommended C# extensions and .NET 10 SDK installed, select GUI (Debug x64), CLI (Debug x64), or Compatibility tests (Debug x64) and press F5. Each configuration builds its target first and uses the repository root as its working directory. The CLI defaults to printing configuration; edit args in .vscode/launch.json to reproduce a specific command.

Managed breakpoint locations include DesktopWorkflow, BuildPipeline, SurcodeMlpProvider and MlpEncoder. Stepping inside the native C encoder requires a separate native debugger and matching symbols. Do not change production floating-point options merely to debug it.

## Packaging

Default publishing creates a framework-dependent Windows x64 GUI single EXE, requiring .NET 10 Desktop Runtime x64. Only runtime components are embedded; instructions, licenses, example configuration and component notices accompany the EXE in one release ZIP. Compressed assets are embedded only when publishing. Debug bin/obj, PDBs, the CLI and F5 remain available.

```bat
tools\win-build\build-all.cmd ^
  --source "D:\dev\winbuild\src" ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --media-runtime "build\media-native-shared" ^
  --image-author "build\image-author-shared"

rem Optional diagnostic folder with the same native builds
tools\win-build\build-all.cmd ^
  --source "D:\dev\winbuild\src" ^
  --prebuilt "D:\dev\winbuild\menu-bin" ^
  --media-runtime "build\media-native-shared" ^
  --image-author "build\image-author-shared" --directory --include-cli --output "D:\release-diagnostics"
```

Media and menus use one shared FFmpeg build and one DLL set in menu-bin. DVDA_MEDIA_NATIVE_DIR may point there; DVDA_IMAGE_NATIVE_DIR points to sibling image-native. The cache defaults to %LOCALAPPDATA%/DVD-Audio-Maker/runtime; tests may isolate it with DVDA_BUNDLE_CACHE_ROOT. See [Windows builds](../tools/win-build/README.en.md) and [onefile design](ONEFILE-PUBLISH.md).

The historical validation below does not describe current package sizes or runtime defaults.

## Validation: 2026-10-02

- Standard package: build/gui-only-x64-release/DVD-Audio-Maker, 423 files, 357,490,318 bytes; ZIP 172,946,471 bytes. All five CLI runtime/launcher files are absent.
- Optional diagnostics: build/gui-cli-diagnostics-x64-release/DVD-Audio-Maker. CLI launchers and separate documentation are included and work.
- 39/39 targeted checks and 107/107 Debug x64 compatibility tests passed.
- All three VS Code preLaunchTask build commands were executed; debug targets and portable PDBs exist. Both developer scripts work, including quoted paths and failure exit codes. An interactive breakpoint session inside VS Code was not performed.
- Release and development build directories are isolated; release application assemblies have no CodeView or embedded PDB debug records.
- Chinese and English GUI startup uses the bundled CoreCLR with global runtime search paths isolated. The GUI-only package encodes mono, stereo and six-channel fixtures with complete MLP byte identity against the previous package.
- The native encoder fingerprint, tools, fonts and menu assets are unchanged; manifests and ZIP contents pass validation.

See [gui-only-validation.json](gui-only-validation.json).

## Image component debugging

Menu development also needs a matching author, menu-bin DLLs, image-native and data/menu. Set DVDA_AUTHOR in a test profile to the new package's menu-bin/dvda-author-dev.exe and DVDA_AUTHOR_SRC to its data directory. Keep the complete runtime together. Run gui-debug.cmd --config <test-profile>; DVDA_IMAGE_NATIVE_DIR and DVDA_MEDIA_NATIVE_DIR can point to the package's image-native/media-native directories. Rebuild the three C modules with tools/win-build/build-menu-runtime.py and integrate them with build-image-author.py --menu-runtime <module-directory> --ffmpeg-runtime <menu-profile-prefix>.

The GUI and native author share image-native/dvda-image.dll. Copy image-native and its sibling menu-bin/fonts from a verified release, or set DVDA_IMAGE_NATIVE_DIR to the complete image directory. Ordinary C# changes do not require rebuilding ImageMagick. See [image processing](INPROCESS-IMAGES.en.md) and [native builds](../tools/win-build/README.en.md).
