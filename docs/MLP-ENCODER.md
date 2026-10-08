# MLP 编码

正常的 `surcode-batch` 流程通过流式 x64 C ABI 调用项目内置的 MLP 编码器。它不会启动 `surcodemlp.exe`、eac3to、MLP 编码子进程或外部 FFmpeg 可执行文件。

当前链路：

```text
音源 -> 内置媒体 DLL -> 整数 PCM WAVE -> mlp_encoder.dll
     -> 只读校验 -> MLP 缓存 -> DVD 制作
```

编码器源码和固定的 Windows x64 DLL 位于 `native/mlp-encoder`。发布包已经包含经过验证的 DLL；重新构建使用该目录的 `build.cmd`。GUI 和 CLI 共用同一套 Rust 流程，不存在独立的 MLP 编码 EXE。

## 配置

程序只读取版本化 JSON 配置方案。默认路径是 `%LOCALAPPDATA%/DVD-Audio-Maker/settings.json`，开发时可以用 `--profile` 指定其他 JSON 文件。旧的 `config.env` 格式及其解析器已经删除。

相关字段示例：

```json
{
  "DVDA_MLP_SOURCE": "surcode-batch",
  "DVDA_MLP_SURCODE_SAMPLE_RATE": "48000",
  "DVDA_MLP_SURCODE_BITS": "24",
  "DVDA_MLP_METADATA_CONTEXT": "",
  "DVDA_MLP_EXTERNAL_DIR": ""
}
```

`DVDA_MLP_JOBS` 为兼容旧配置而保留但会被忽略。生产编码自动使用 `min(2 × 逻辑处理器数量, 工作项数量)`；并行只发生在独立音轨之间，单首音轨的编码顺序和输出字节不变。

DVD-Audio 采样率支持 44.1、48、88.2、96、176.4 和 192 kHz，整数位深支持 16、20、24 bit。声道上限遵循 DVD-Audio 布局：较低采样率最多六声道，176.4/192 kHz 最多两声道。将 `DVDA_MLP_SOURCE` 设为 `lpcm` 可选择 LPCM 编码。

编码器在序列化前接收规范化 PCM 和元数据，输出后不修改编码字节。要得到逐字节完全一致的文件，PCM、格式参数、辅助元数据上下文和编码行为都必须一致。复现历史文件时，可以通过 `DVDA_MLP_METADATA_CONTEXT` 提供对应的 `MSCTX001` 上下文。已有 MLP 可通过 `DVDA_MLP_EXTERNAL_DIR` 只读导入；导入文件会校验，不会重新编码。

## 运行时和缓存

发布 ZIP 是 Windows x64 原生包。媒体和图像功能由随包提供的项目构建 DLL 完成。开发测试可以显式指定已经验证的原生目录；组件缺失时报告错误，不静默回退外部程序。

缓存键包含音源内容、编码器身份、PCM 转换策略、格式参数、元数据上下文和输出内容。缺少这些来源信息的旧缓存会重建。失败或取消的任务不会发布不完整的 MLP 文件。

## 验证

在仓库根目录运行：

```powershell
cargo fmt --all -- --check
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
```

原生 ABI 测试覆盖编码器、PCM 比较、格式解析和失败路径。原版文件逐字节对照记录在 `native/mlp-encoder/mlpencoder-validation.json`，它用于固定编码器产物验收，不是通过事后修补输出获得的一致性。

编码器会拒绝不支持的布局、浮点 PCM、有效位丢失、损坏输入以及超过当前 RIFF/WAVE 32 位长度边界的音轨。超大 AU 的无损回退见 `docs/MLP-OVERSIZE-FIX.md`。
