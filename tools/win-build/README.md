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

完整 author 由 Rust 实现：应用进程内调用，独立 Rust CLI 复用同一流程。`build-image-author.py` 是 `build-rust-author.py` 的兼容入口，旧 C producer、源码和冻结二进制已清理。AOB、管理表、菜单/静图和 ISO 的验收见 [author 迁移记录](../../docs/RUST-AUTHOR-MIGRATION.md)，黄金数据的历史来源摘要仍保留。

## 原生输入

打包器需要已验证的 x64 媒体/图像构建记录与 `build/rust-author-production`，并验证摘要、大小、PE 架构、DLL 闭包及字体/菜单素材。图像记录必须标识 Rust 实现；本机验收使用 `build/rust-image-runtime`。MLP 编码器直接编入 Rust 应用，`--encoder-runtime` 仅用于认证可选对照产物，不分发编码器 DLL。FFmpeg 和 ImageMagick 命令行程序不是运行时输入，第三方库由 Rust bridge 调用；媒体/图像开发 adapter DLL 不进入静态链接发布包。

格式解析、MLP CRC/奇偶校验、PCM 比较及只读 AOB 成品校验已经内置为 Rust 实现，不需要格式或校验 DLL。打包不再接受 `--formats-runtime` 输入。

## 发布 ZIP

```bat
tools\win-build\build-all.cmd ^
  --media-runtime build\media-native-shared ^
  --image-runtime build\rust-image-runtime ^
  --image-author build\rust-author-production ^
  --prebuilt build\release-menu-final ^
  --version v1.0
```

命令通过 `dvda-toolchain package` 生成 `DVD-Audio-Maker-v1.0-win-x64.zip`。发布包只包含内嵌必要 DLL、菜单资源和字体的 GUI EXE，以及旁置用户文档，不包含 .NET、开发 CLI、PDB、构建 JSON 或本机配置。三语用户 README、许可证与两个 NOTICE 均在 EXE 旁边；运行组件不作为旁文件分发。
