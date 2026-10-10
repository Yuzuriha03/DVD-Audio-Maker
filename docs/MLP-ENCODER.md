# MLP 编码

正常的 `surcode-batch` 流程调用直接链接到主程序的 Rust MLP 编码器。它不会加载编码器 DLL，也不会启动 `surcodemlp.exe`、eac3to、MLP 编码子进程或外部 FFmpeg 可执行文件。

当前链路：

```text
音源 -> 主程序内置媒体桥接 -> 整数 PCM WAVE -> 主程序内置 Rust MLP 编码器
     -> 只读校验 -> MLP 缓存 -> DVD 制作
```

Rust 编码器源码位于 `rust/crates/dvda-mlp`，以 rlib 链接到 GUI/CLI。原 C 源码和冻结 Windows x64 DLL 位于 `native/mlp-encoder`，仅用于差分验收。发布包为 GUI-ONLY，不包含 CLI 或编码器 DLL。

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

发布 ZIP 是 Windows x64 GUI-ONLY 原生包，只包含 GUI EXE 和用户文档。媒体、图像及菜单桥接直接链接到主程序；必要的第三方资源内嵌并在启动时释放到版本化缓存。开发测试可以显式指定已经验证的原生目录；组件缺失时报告错误，不静默回退外部程序。

缓存键包含音源内容、编码器身份、PCM 转换策略、格式参数、元数据上下文和输出内容。缺少这些来源信息的旧缓存会重建。失败或取消的任务不会发布不完整的 MLP 文件。

## Rust 逐函数迁移状态

迁移以冻结原 C 编码器为唯一基准，最终验收必须在相同 PCM、参数和 metadata 下比较原始 MLP 文件的全部字节及 SHA256；解码一致不能替代这项验收。

当前矩阵执行、缩放、搜索、预测、参数、restart 和 substream 模块已接入 Rust 编码主链。restart 与 substream 各通过 12,000 组 C 序列化差分，主编码调度及直接链接生产接线已完成。当前验收范围已经用户确认通过，并授权更新 v1.0 GUI-ONLY 发布。

矩阵 analysis init/reset/add、decorrelate、select/append，以及 downmix design/render/PCM 校验/plan/process 已逐函数移植；实际 C DLL 差分覆盖 12,000 组、x87 53/64 位精度，logl 扩展精度结果按 10 字节比较。上述模块已接入生产主链；整文件一致性由下述冻结 C 全字节差分验收独立确认。

输出队列 init/dispose/release/push/finish 已通过 64 组原 C 差分（含 lookahead 边界、双子流和输出失败）；rate 通过 24,000 组状态差分，metadata stamp 通过 12,000 组位流和状态差分。`prepare_matrix` 已通过 256 组 PCM、矩阵、scale、prediction、pool 及调度状态差分；`flush_interval` 直接调用原 C 静态函数的 256 组差分比较输出 words 和 rate/pending 状态，包含 oversized 回退、FIFO 拒绝与输出回调失败。当前 prepare 用例还包含触发末短区间门限问题的 16 区间历史状态回归。

主 `encode` 已接通 prepare→通道预测→stamp→flush→queue→finish 的测试链，1,536 组直接原 C 主函数对照在 48 kHz、1–6 声道、16/20/24 bit、不同长度/restart/cycle/矩阵/scale/search 下输出 words 一致，并包含同一显式 metadata packet。通道循环已并回主 `encode`，上述差分重新通过。输入抽象现支持有界 callback 读取，按原 C 顺序进行碎片读取、signed-24/声明位深校验、声道重排及末 AU 补零；不再要求主调度持有整条音轨。

新增冻结原 DLL 的真实流式 ABI 对照 `frozen_stream_bytes`：504 组合法标准格式覆盖全部六种采样率、16/20/24 bit、合法的 1–6 声道及 1/41/641/1282/2560/4097 帧，双方读取回调每次最多 17 帧。Rust host adapter 的完整原始 MLP 字节、input/encoded frames、AU 数和输出字节数与冻结 C DLL 完全一致；major header 由 Rust 自行生成。host adapter 保存/恢复 x87 和 SSE 控制状态，回调切回 caller 环境。运行该测试必须显式设置 `MLP_FROZEN_REFERENCE` 指向冻结原 DLL。

x64 Rust 编码器已通过 `dvda-mlp` 的 rlib 直接编入 GUI/CLI 主程序，`dvda-core` 默认启用 `rust-mlp`；正常运行不查找、不加载编码器 DLL。冻结 C DLL 仅保留为测试 oracle，显式 `--no-default-features` 的旧后端仅用于对照。新增 116 个冻结宿主输出集成回归，使用不存在的编码器 DLL 路径执行正式入口，全部匹配冻结输出长度和 SHA256。重新构建的 GUI Release EXE 包含 MLP 编码函数，PE 导入表仅系统库，无编码器 DLL。当前已授权提交和 GUI-ONLY 发布。冻结原 DLL 对照现在直接调用 Rust 的真实 `mlp_encode_stream`、layout/depths 和 groups 导出，而不是仅测试 host helper。Release 验证中，504 组标准格式、528 组全部 assignment/分组位深格式及独立组输入用例均通过完整原始字节、显式 SHA256 和结果计数比较。独立组输入覆盖等采样率与半采样率、16/20/24 bit 及短尾/跨 interval 输入；修正了组回调的零容量、超容量、EOF 和插值 FP 环境。扩展后的 `frozen_group_bytes` 覆盖全部 19 种双组 assignment、44.1/48/88.2/96 kHz、六种合法位深对（16/16、20/16、20/20、24/16、24/20、24/24），以及 C 支持的同采样率和 88.2/96 kHz 半采样率关系。每种格式使用 2/82/642/4098 帧，共 2736 个用例；Debug 和 Release 实测全部通过冻结 C 原始字节、SHA256 和结果计数比较。原 C 不允许第二组位深高于第一组，也不允许 44.1/48 kHz 半采样率，不将这些组合计入合法矩阵。新增 `frozen_group_signal_boundaries`：在同一 684 种合法双组格式上，将静音、交替满幅极值、首尾脉冲、相关声道、反相关声道和固定种子随机 PCM，与 AU 边界及 8-AU 分析区间边界前/上/后的六种长度完整交叉，共 24624 个用例。Release 实测全部通过原始字节、SHA256 和结果计数比较；与原 2736 个格式用例合跑也通过。该覆盖不替代长轨、更多随机种子、可变 restart interval 或生产失败路径验收。

首轮 Release 小输入批次计时（编码调用本身，不含输入构造与哈希）为：标准格式 C 882 ms / Rust 606 ms，Rust/C 0.686；layout/depths C 1085 ms / Rust 756 ms，Rust/C 0.697。这只是初步结果，不是多轮预热、长音轨及峰值内存的最终性能结论。错误/reentry/并发、显式及碎片 metadata 和组压力测试已通过下述严格验收；更广泛长轨与峰值内存性能仍未作最终结论。最终验收后继续公平比较耗时、吞吐量、实时倍速及峰值内存；不得以改变行为换取速度。

整 DLL groups 压力测试已升级为完整原始字节与 SHA256 比较。曾出现的 96000 Hz、assignment 18、group2 16 bit、半采样率、9602 帧用例第 37406 字节差异已解决：相同重建 PCM 的逐区间 prepare 对照确认，最后一个短区间的矩阵去相关 gain 门限在 C 中使用 x87 扩展精度常量，而 Rust 原先先将 `0.1` 舍入为 binary64，边界比较改变了矩阵枢轴顺序。Rust 现保留 C 的扩展精度常量；新增可复现的 16 区间 prepare/analysis/scale/matrix/predictor/pool 状态回归，不依赖临时 PCM 文件。全部 228 个长分组压力用例通过原始 MLP 逐字节和 SHA256 比较，并保留插值 PCM、header/parity、并发和错误路径校验。

多轮 Release 性能测试：48 kHz、24 bit、双声道、48000 帧，预热一轮、交替顺序测量五轮，C 中位 43.889 ms（22.78 倍实时），Rust 30.105 ms（33.22 倍实时），Rust/C 0.686；每轮原始字节及 SHA256 一致。该单一输入结果不能替代长音轨、组输入与峰值内存结论。

C 模块差分测试必须显式设置对应的 reference 环境变量；普通测试显示通过不能证明这些 DLL 已被执行。`build-runtime.ps1` 构建并设置 planning、entropy、timing、group、queue 和 prepare reference；新增 queue/prepare/flush 差分缺少环境变量时会直接失败。完整脚本当前通过 35 项测试、142 个标准压力用例和 228 个分组压力用例，严格比较原始字节和 SHA256；不能将局部差分或解码一致单独当作发布许可。

## 验证

在仓库根目录运行：

```powershell
cargo fmt --all -- --check
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- --include-ignored
cargo clippy --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --workspace --offline -- -D warnings
```

原生 ABI 测试覆盖编码器及其失败路径；Rust 测试覆盖 PCM 比较、格式解析和只读成品校验。原版文件逐字节对照记录在 `native/mlp-encoder/mlpencoder-validation.json`，它用于固定编码器产物验收，不是通过事后修补输出获得的一致性。

编码器会拒绝不支持的布局、浮点 PCM、有效位丢失、损坏输入以及超过当前 RIFF/WAVE 32 位长度边界的音轨。超大 AU 的无损回退见 `docs/MLP-OVERSIZE-FIX.md`。
