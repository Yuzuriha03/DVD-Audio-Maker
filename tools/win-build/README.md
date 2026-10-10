# Windows x64 构建与发布

当前产品是 Windows x64 Rust 原生 GUI，运行时不需要 .NET。重建原生组件需要 Rust、现有 MSYS2/MinGW-w64 GCC 和 Python。

## Rust 构建

```bat
cargo build --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline
cargo fmt --manifest-path rust\Cargo.toml --all -- --check
cargo clippy --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
cargo test --manifest-path rust\Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
```

`gui.cmd`、`gui-debug.cmd`、`cli.cmd`、`build.cmd` 和 `verify.cmd` 都调用 Rust workspace。配置只使用 JSON 方案和 `--profile`；不支持 `config.env`、`DVDA_CONFIG` 和 `--config`。

## 原生输入

打包器需要已验证的 x64 目录：`build/media-native-shared`、`build/image-native`、`build/rust-author-current`、菜单资源目录，以及 `build/mlp-encoder`（可用 `--encoder-runtime` 指定）。编码器目录必须包含验收后源码构建产生的 Rust ABI v1 `encoder-build.json` 和唯一的 `mlp_encoder.dll`；打包器验证摘要、大小、PE 架构及依赖闭包，不回退到旧 C DLL。当前编码器迁移尚未验收，发布流程因此暂不可完成。FFmpeg 和 ImageMagick 命令行程序不是运行时输入；所需库通过项目 C ABI 加载。旧 MLP C 源码和冻结 DLL 仅在迁移验收前保留。

格式解析、MLP CRC/奇偶校验、PCM 比较及只读 AOB 成品校验已经内置为 Rust 实现，不需要格式或校验 DLL。打包不再接受 `--formats-runtime` 输入。

## 发布 ZIP

```bat
tools\win-build\build-all.cmd ^
  --media-runtime build\media-native-shared ^
  --image-runtime build\image-native ^
  --image-author build\rust-author-current ^
  --encoder-runtime build\mlp-encoder ^
  --prebuilt build\release-menu-final ^
  --version v1.0
```

命令通过 `dvda-toolchain package` 生成 `DVD-Audio-Maker-v1.0-win-x64.zip`。发布包只包含内嵌必要 DLL、菜单资源和字体的 GUI EXE，以及旁置用户文档，不包含 .NET、开发 CLI、PDB、构建 JSON 或本机配置。三语用户 README、许可证与两个 NOTICE 均在 EXE 旁边；运行组件不作为旁文件分发。
