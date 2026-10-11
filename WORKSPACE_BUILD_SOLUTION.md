# Windows MSVC 构建说明

项目的唯一受支持 Rust 目标是 `x86_64-pc-windows-msvc`。所有入口脚本、Cargo 配置和工具链校验必须使用该目标；GNU target、MinGW 链接器和 `.a` vendor 归档不属于当前构建链。

## 可验证的核心检查

在没有外部媒体依赖时运行：

```powershell
cargo test --manifest-path rust/Cargo.toml --workspace --no-default-features --offline
cargo check --manifest-path rust/Cargo.toml -p dvda-cli --no-default-features --offline
```

## 完整 direct-bridges 构建

默认 CLI/GUI 会启用 `direct-bridges`，需要与 MSVC ABI 匹配的：

- FFmpeg shared import libraries and headers via `DVDA_FFMPEG_PREFIX`;
- ImageMagick static libraries and headers via `DVDA_MAGICK_WORK`;
- MSVC-built menu vendor libraries via `DVDA_MENU_NATIVE_DIR`.

缺少任一输入时，build script 应直接报出缺失项；不能生成空 ABI 布局，也不能跳过链接后继续构建。当前工作区没有把旧 GNU 归档伪装成 MSVC 依赖。

## 迁移状态

Rust author、DVD authoring 和 ISO writer 已在 Rust 中；A/B/C 优化的可复用 API 和有界流水线已实现并有单元测试。真实媒体构建与发布包必须在 MSVC 原生依赖重新构建后再验收。
