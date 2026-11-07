# 性能基线与验证

本文只记录可复现的测量结果，不使用“预期提速”或未经验证的百分比。当前完整报告由 `tools/win-build/benchmark-report.py` 生成，使用说明和测量边界见 [P0 性能基线报告](BENCHMARKS.md)。

## 构建与实现基线

- Release 使用 `opt-level=3`、fat LTO、单 codegen unit、strip debuginfo 和 `panic=abort`。
- 发布构建不强制 `target-cpu=native`，避免牺牲 x64/ARM64 可移植性。
- 并行使用有界 std 线程池，不使用 rayon。
- PCM/MLP 采用流式输入和有界缓冲回收；菜单图像使用任务级解码与链式缩放缓存。
- 原生依赖仅为当前工作流需要的 FFmpeg 组件和 soxr；不包含 ImageMagick 或原生 zlib。

这些配置本身不构成性能收益声明，必须与固定输入的前后基准比较。

## 当前 x64 微基准

报告：`build/benchmarks/baseline-x64.json`。
提交：`e8c4ae4b8f30ff02e1e14942028181a987e2ce4d`。
目标：`x86_64-pc-windows-msvc`，每项独立运行三轮，取中位数。

| 基准 | 中位耗时 | 临时空间采样峰值 |
| --- | ---: | ---: |
| inspection_progress_callback_benchmark | 1.454 ms | 0 B |
| normalization_throughput | 40.698 ms | 83,886,124 B |
| packing_32_throughput | 45.379 ms | 117,440,563 B |
| pcm_comparison_throughput | 27.778 ms | 134,217,730 B |

对应的进程峰值工作集分别约为 6 MiB、102 MiB、198 MiB 和 69 MiB。临时空间只按已知基准目录每 50 ms 采样，不是整个工作流或系统峰值。

复现命令：

    .venv\Scripts\python.exe tools\win-build\benchmark-report.py --output build\benchmarks\baseline-x64.json

报告包含版本、提交、架构、三轮结果、输入/输出 SHA-256、逐项日志和测量限制。任何后续优化都应使用同一输入比较中位数，不能用不同机器或不同输入直接比较这些数值。

## MLP 熵参数分析移位（2026-11-07，未提交工作区）

基线为 `b9d03f7d08d4290b06d477106014c5e4c1a59ddc` 加测试代码，优化项仅把 `dvda-mlp/src/entropy.rs` 中两个已限定移位范围的 2 的幂除法改为算术右移。x64 Release 每项独立运行三轮；`entropy_select` 为 10,000 个 160 样本块，样本生成和参数选择均包含在计时内。完整数据见 `build/benchmarks/entropy-shift-x64.json`。

| 测量项 | 基线三轮 ms | 优化三轮 ms | 中位数变化 |
| --- | --- | --- | ---: |
| entropy_select | 10.6934 / 10.4544 / 10.9650 | 3.2251 / 2.9508 / 3.6260 | 10.4544 → 3.2251（−69.15%） |
| entropy_word 控制项 | 11.0414 / 11.1165 / 10.4536 | 10.5885 / 10.8877 / 10.6159 | 11.0414 → 10.6159（−3.85%，无独立收益声明） |

两组的确定性输出累加值均为 `7966540325572`。新增普通测试用边界值和 10,000 个确定性样本比较移位与原有向负无穷除法；54 个 dvda-mlp 单元测试、严格 Clippy 和 116 个固定 MLP 长度/SHA-256 用例通过。该百分比只描述上述微基准，不代表完整专辑编码提速。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-mlp --release entropy_arithmetic_throughput '--' '--ignored' '--nocapture'

同期对 `syntax/residual.rs` 的逐样本移位试验中位数从 10.4287 ms 变为 11.0426 ms（+5.89%），没有保留该生产改动；`Coding::word` 的两处无独立收益改动也已撤回。把声道参数预计算并提升切片的三轮试验中位数从 9.9466 ms 变为 10.1146 ms（+1.69%），同样已撤回，数据见 `build/benchmarks/substream-prepared-x64.json`。

## 封面十六进制直接比较（2026-11-07，未提交工作区）

`dvda-core/src/conversion.rs` 不再为源封面创建双倍长度的十六进制字符串，而是逐字节与 FLAC `ImageHex` 的两个字符进行大小写不敏感比较。基准在同一进程内交替测量旧格式化路径和新直接比较路径，每轮各处理 10 × 1 MiB 确定性数据，独立运行三轮。

| 测量项 | 三轮总耗时 ms | 中位数 |
| --- | --- | ---: |
| 旧格式化与字符串比较 | 79.2214 / 78.7262 / 79.9586 | 79.2214 ms |
| 新直接比较 | 10.2815 / 9.7435 / 9.8802 | 9.8802 ms（−87.53%） |

新路径不分配十六进制字符串。全部 256 个字节值、大小写、长度、非法字符和首尾差异测试通过；167 个 dvda-core 单元测试、严格 Clippy 以及真实 ALAC/PNG 封面转换验收通过。报告见 `build/benchmarks/cover-hex-x64.json`。该结果不代表完整转换耗时，音频解码、编码和其他 I/O 不在此微基准内。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-core --release cover_hex_comparison_throughput '--' '--ignored' '--nocapture'

## MLP 缓存加载排序去重（2026-11-07，未提交工作区）

`dvda-core/src/cache.rs` 将逐条扫描已有键的 O(n²) 重复检测改为对“原始索引 + 键引用”排序后检查相邻项，并直接返回原 JSON Map。条目形状改为借用反序列化，去掉每条目两次深拷贝；`find_mlp`/`record_mlp` 的路径文本转换移到比较循环外。原始插入顺序和“先校验当前项、再报告重复”的错误优先级保持不变。

5,000 条确定性有效缓存的完整 JSON 加载（含读取、解析、形状校验和重复检测）独立运行三轮：

| 实现 | 三轮耗时 ms | 中位数 |
| --- | --- | ---: |
| 原线性嵌套扫描 | 360.8798 / 369.0938 / 368.6136 | 368.6136 ms |
| 排序相邻检测 | 17.9594 / 17.4200 / 20.9545 | 17.9594 ms（−95.13%） |

重复项与后续损坏项的错误优先级、重复项自身损坏、BMP/补充平面 ordinal 大小写、ß/STRASSE 区分和原插入顺序测试通过；168 个 dvda-core 单元测试和严格 Clippy 通过。报告见 `build/benchmarks/mlp-cache-load-x64.json`。结果只代表该合成缓存规模，真实缓存预检和 ARM64 原生执行未运行。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-core --release mlp_cache_load_throughput '--' '--ignored' '--nocapture'

## MLP access unit 缓冲（2026-11-07，未提交工作区）

实际写入发生在 `mlp_substream_put` 的私有 `Bits` 中，因此只给外层 writer 预留无效。现在该私有缓冲通过 `Bits::with_capacity(512)` 创建，保留超出容量时的正常增长语义；输出队列新增接管 `Vec<u32>` 所有权的生产入口，去掉入队时的第二次全量复制。切片入口保留为测试参考。

10,000 次完整 160 帧、双声道子画面序列化的前后结果：

| 实现 | 三轮耗时 ms | 中位数 | 累计容量 |
| --- | --- | ---: | ---: |
| 原增长缓冲 | 21.2973 / 20.7216 / 21.2739 | 21.2739 ms | 5,120,000 words |
| 768-word 预留（未保留） | 17.5266 / 17.8934 / 19.8568 | 17.8934 ms（−15.89%） | 7,680,000 words |
| 512-word 预留（保留） | 17.2776 / 18.9227 / 18.5412 | 18.5412 ms（−12.85%） | 5,120,000 words |

两种实现的输出均为 4,950,000 words；512-word 方案没有增加该基准的最终容量。队列另以 10,000 个合法 495-word AU 做配对测量，复制路径三轮为 0.7309 / 0.7681 / 0.8206 ms，所有权路径为 0.4268 / 0.4433 / 0.3729 ms，中位数从 0.7681 ms 降至 0.4268 ms（−44.43%）。队列基准隔离单次入队，不代表完整编码提速。

54 个 dvda-mlp 单元测试、严格 Clippy 和 116 个固定 MLP 长度/SHA-256 用例通过。详细数据见 `build/benchmarks/au-buffer-x64.json`；ARM64 原生执行未运行。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-mlp --release au_substream_buffer_throughput '--' '--ignored' '--nocapture'
    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-mlp --release queue_ownership_throughput '--' '--ignored' '--nocapture'

## 矩阵原地应用试验（2026-11-07，未保留）

对 `analysis/matrix.rs` 的私有 interval 工作缓冲跳过进入/返回双拷贝，完整 10,000 次双声道 160 帧调用的三轮基线为 11.1668 / 11.3762 / 11.4833 ms，原地入口为 12.0617 / 12.1924 / 11.2329 ms，中位数从 11.3762 ms 变为 12.0617 ms（+6.03%）。输出累加值均为 370000，既有 bypass 往返和溢出回滚测试通过，但没有吞吐收益，生产与基准改动均已撤回。报告见 `build/benchmarks/matrix-in-place-x64.json`。

## FLAC PICTURE 原始字节读取（2026-11-07，未提交工作区）

`dvda-core/src/formats.rs` 新增内部原始图片读取入口，返回元数据、原 PICTURE block 及图片偏移，不额外复制图片。现有 `flac.picture` JSON API 仍在末尾生成相同的 `ImageHex`；`conversion.rs` 改用原始字节直接比较，封面转换路径不再创建任何十六进制字符串。

配对基准在同一 1 MiB PICTURE 文件上分别执行 10 次完整读取和比较，独立运行三轮：

| 路径 | 三轮总耗时 ms | 中位数 |
| --- | --- | ---: |
| JSON/ImageHex 与文本比较 | 22.1149 / 23.0837 / 22.7492 | 22.7492 ms |
| 原始 block 与字节比较 | 6.9435 / 7.0496 / 6.9044 | 6.9435 ms（−69.48%） |

原始字节 API、既有 JSON 字段、全部截断/oversize/UTF-8/trailing-bytes 测试通过；168 个 dvda-core 单元测试、真实 ALAC/PNG 封面转换验收和严格 Clippy 通过。报告见 `build/benchmarks/flac-picture-raw-x64.json`。该基准包含文件读取与 FLAC 元数据解析，但不包含音频解码或编码；ARM64 原生执行未运行。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-core --release --test flac_metadata flac_picture_raw_data_throughput '--' '--ignored' '--nocapture'

## MLP 缓存逐轨有序索引（2026-11-08，续航模式）

用户已开启续航模式，测量期间未修改任何电源选项。以下基线和优化结果只在本节内部比较，不与此前电源状态下的加载数据直接比较。

`MlpCache` 现在由 Map 和独立有序键组成：加载复用已有排序结果并克隆有序键，查找二分 ordinal 比较后读取 Map；替换通过有序索引中的原键更新，新增键分别插入有序索引和 Map。保存只序列化 Map，不包含索引。

| 5,000 条缓存测量 | 基线中位数 | 索引中位数 | 变化 |
| --- | ---: | ---: | ---: |
| 完整加载（含有序键克隆） | 29.5420 ms | 34.0972 ms | +15.42% |
| 5,000 次查找 | 351.3565 ms | 7.4923 ms | −97.87% |
| 99 次大小写替换 | 0.3683 ms | 0.2275 ms | −38.23% |

该实现以小幅加载开销换取重复查找收益。原键大小写、Map 插入顺序、保存 JSON、ordinal 比较器、错误优先级和 Send+Sync 均通过测试；170 个 dvda-core 单元测试和严格 Clippy 通过。报告见 `build/benchmarks/mlp-cache-index-x64.json`。真实缓存预检、非 ASCII 路径性能和 ARM64 原生执行未运行。

复现命令：

    & rust\target\x86_64-pc-windows-msvc\release\deps\dvda_core-cb01fa3d20f42896.exe mlp_cache_ --ignored --nocapture

## ALAC 全精度 PCM 校验流式化（2026-11-08，未提交工作区）

`dvda-core/src/conversion.rs` 复用既有 `verify_audio_stream` 有界比较器，不再把源侧和转换侧分别完整写成 `source.pcm`/`converted.pcm` 后读回。比较仍要求长度和全精度样本一致，不允许补帧；原有全精度差异与取消错误文案保留，worker 数纳入音频内存预算。目标 FLAC 的 MD5 仍按原格式上报；被全精度比较覆盖的源侧 MD5 解码已删除，减少一次完整源音频解码。

原生 ALAC→FLAC 端到端用例通过标签、封面、PCM、批处理失败与取消验收；新增 16/24 位原生用例覆盖字符串/数字元数据、样本差异和临时 PCM 文件不存在断言。170 个 dvda-core 单元测试通过、13 项按条件忽略，严格 Clippy 通过。这些结果证明功能与临时 I/O 消除；完整旧/新性能对照另行记录，不预先声明提速百分比。

Release 配对基准使用 10 秒、48 kHz、双声道、24-bit 确定性 PCM；旧路径包含两路解码、临时 PCM 写入和读回，新路径包含有界双线程流式比较。预热后交替测量六轮：

| 轮次 | 落盘比较 ms | 流式比较 ms |
| --- | ---: | ---: |
| 1 | 186.3814 | 99.7712 |
| 2 | 172.4243 | 91.1028 |
| 3 | 175.3039 | 91.3658 |
| 4 | 164.7535 | 82.3946 |
| 5 | 236.5776 | 77.5100 |
| 6 | 212.8566 | 68.9076 |
| 中位数 | 180.8427 | 86.7487（约 2.084×） |

每轮流式路径都快于对应的旧路径，两种比较均成功。该结果只表示上述 PCM 校验基准，不包含完整 ALAC 转换、源侧 MD5 删除带来的额外收益或真实专辑耗时。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-core --test conversion_native converter_preserves_tags_cover_pcm_and_batch_failure_semantics '--' '--ignored' '--nocapture'
    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-core --lib streaming_pcm_verification_preserves_precision_and_error_contract '--' '--ignored' '--nocapture'
    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-core --lib pcm_streaming_comparison_throughput --release '--' '--ignored' '--nocapture'

## FLAC 元数据按需读取（2026-11-08，未提交工作区）

`dvda-core/src/formats.rs` 的 comments、JSON PICTURE 和原始 PICTURE 查询改为逐块读取：只把目标 metadata block 读入内存，其他块用 seek 跳过，最后一个 metadata block 后的音频区不读取。现有 `flac.replace_picture` 暂未改动，避免把查询优化扩大为写入路径重写。

测试用计数 reader 证明：8 MiB 音频区不产生读取；4 MiB 非目标 padding block 不进入内存；返回 JSON、ImageHex、原始 block 和图片偏移保持原契约。另覆盖截断错误优先级、oversize、UTF-8、trailing bytes 和 Windows 句柄关闭。flac_metadata 的 9 项测试通过、1 项忽略，严格 Clippy 通过。

Release 真实文件基准使用 64 MiB 音频区、8+2 MiB 非目标 padding 和 6 MiB 图片；旧 `fs::read` 整文件路径与新流式路径交替三轮。净窗复测以 50 ms 采样确认测量期间 `cargo`/`rustc` 进程数为 0：

| 查询 | 旧路径三轮 ms | 流式路径三轮 ms | 中位数变化 |
| --- | --- | --- | ---: |
| comments | 61.0214 / 54.5533 / 58.3650 | 0.2997 / 0.4047 / 0.5715 | 58.3650 → 0.4047（约 144.2×） |
| 原始 PICTURE | 53.0773 / 66.8431 / 57.4407 | 3.6761 / 3.9969 / 4.0386 | 57.4407 → 3.9969（约 14.37×） |
| JSON PICTURE | 73.6354 / 76.0325 / 82.1219 | 18.0899 / 23.2354 / 20.1953 | 76.0325 → 20.1953（约 3.76×） |

返回 block、JSON 与图片字节一致，三项中位数均明确下降。结果只表示该文件和元数据形状，不外推为所有磁盘上的固定倍率。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-core --lib flac_streaming_bench::flac_streaming_ab_benchmark --release '--' '--ignored' '--nocapture'

## 菜单背景解码与 select 编码去重（2026-11-08，未提交工作区）

`dvda-author/src/menu.rs` 通过调用方 `ImageCache` 获取菜单 blank，绘制前只克隆栅格，避免每页重复完整解码；image 与 select 内容相同，select 改为复制已编码 image，省去第三次 PNG 编码。输出顺序仍为 image→highlight→select，复制前后取消检查和失败清理保留。

缓存与直接解码绘制结果一致、缓存图未被修改、image/select 字节一致及取消清理测试通过；49 个 dvda-author 单元/集成测试通过，严格 Clippy 通过。

Release 同进程完整路径基准使用 32 个 720×576、每页 8 按钮的页面，旧/新顺序交替，每轮重新创建任务级缓存：

| 轮次 | 旧路径 ms | 优化路径 ms |
| --- | ---: | ---: |
| 1 | 2363.807 | 1824.320 |
| 2 | 2162.898 | 1520.552 |
| 3 | 2162.187 | 1742.639 |
| 中位数 | 2162.898 | 1742.639（1.241×） |

另一轮确认测量为旧 2239.714 ms、新 1774.348 ms（1.262×）。32 页 × 3 类图层 × 3 轮全部逐字节一致。结果只表示该菜单掩膜微基准，不代表完整制盘耗时。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-author --release bench_make_masks_old_vs_new '--' '--ignored' '--nocapture'

## MLP 预测零抽头裁剪（2026-11-08，未提交工作区）

`dvda-mlp/src/analysis/prediction.rs` 在每块开头扫描最后非零系数，预测循环只累加有效前缀；满 8/4 抽头保留原遍历快路径，反馈抽头仍从高位到低位累加。扫描而非盲信 `order` 使合成的非零尾系数也保持等价，order、wire 和 fallback 语义不变。

Release 同进程旧/新实现各处理 20,000 个 160 样本块，独立运行三轮；机器上同时存在其他构建，以下原始值含噪声：

| 抽头 | 原实现三轮 ms | 裁剪实现三轮 ms |
| --- | --- | ---: |
| 3+2 | 157.220 / 186.218 / 171.029 | 85.736 / 77.132 / 76.484 |
| 8+4 | 159.327 / 155.342 / 188.257 | 145.986 / 154.806 / 151.195 |

每块输出逐位一致，checksum 均为 20,000。边界、全零、非零尾和 600 个确定性随机用例通过；58 个 dvda-mlp 单元测试、native_float 集成测试、116 个固定 MLP 长度/SHA-256 用例及严格 Clippy 通过。该结果只描述预测微基准，不外推为完整编码收益。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-mlp --release microbench_shortened_traversal_against_reference '--' '--ignored' '--nocapture'
    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-core --test encoder_native linked_mlp_matrix_without_encoder_dll --release

## 字体规范路径与 face 校验缓存（2026-11-08，未提交工作区）

`dvda-image/src/fonts.rs` 增加原始路径到规范路径的缓存，以及每个字体文件的已验证 face 集合。重复加载同一路径和 face 不再执行 `canonicalize()` 或重新创建 `rustybuzz::Face`；新 face 仍需验证，失败时保留缓存项和原 LRU 行为。

Release 使用真实 `DvdaNotoCJK-Regular.ttc`，每轮执行 500 次缓存命中，旧/新顺序交替六轮：

| 轮次 | 原路径与重复校验 ms | 缓存命中 ms |
| --- | ---: | ---: |
| 1 | 168.0159 | 0.2898 |
| 2 | 208.4656 | 0.2782 |
| 3 | 189.1233 | 0.2216 |
| 4 | 177.2919 | 0.2525 |
| 5 | 158.5938 | 0.2126 |
| 6 | 157.8753 | 0.2125 |
| 中位数 | 172.6539 | 0.2371 |

普通测试覆盖“已验证 face 跳过、未验证 face 报错、失败后缓存不丢失”。该基准只表示重复字体加载，不代表完整菜单渲染耗时。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-image --release font_cache_hit_throughput '--' '--ignored' '--nocapture'

## 矩阵倒数乘法试验（2026-11-08，未保留）

通过 const 泛型在同一完整 `mlp_matrix_apply` 中比较原 `sum / 2^qss` 与提升后的 `sum * (1 / 2^qss)`，qss=0..15 的输出和溢出状态一致。完整 10,000 次双声道 160 帧调用的六轮结果：

| 轮次 | 原除法 ms | 倒数乘法 ms |
| --- | ---: | ---: |
| 1 | 18.3207 | 26.7965 |
| 2 | 19.0839 | 29.1932 |
| 3 | 17.4180 | 24.2538 |
| 4 | 17.2410 | 27.1841 |
| 5 | 15.4124 | 24.8241 |
| 6 | 18.8175 | 24.8682 |

新路径每轮均明显更慢，生产实现和对应基准已全部撤销，矩阵文件恢复原状。

## 主窗口 DIB 重绘缓存（2026-11-08，未提交工作区）

`dvda-desktop/src/draw.rs` 为 `paint_window` 增加线程局部、按 HWND 和客户区尺寸键控的一张缓存 DIB。命中时复用 memory DC/bitmap，仍执行背景填充、绘制、BitBlt 和原对象恢复；尺寸变化时重建，`WM_DESTROY` 明确释放。其他 label/panel 控件路径不变。

真实主窗口测试覆盖重复绘制像素一致、尺寸变化、缓存只有一项以及销毁后归零。Release 在 1920×1080 客户区上分别执行 100 次完整 BeginPaint/缓冲/BitBlt/EndPaint 周期，旧/新顺序交替六轮：

| 轮次 | 每次重建 DIB ms | 缓存复用 ms |
| --- | ---: | ---: |
| 1 | 675.5818 | 87.2873 |
| 2 | 170.9447 | 83.1329 |
| 3 | 164.1717 | 83.9308 |
| 4 | 182.2718 | 87.0888 |
| 5 | 163.2427 | 86.7206 |
| 6 | 165.8130 | 85.9160 |
| 中位数 | 168.3789 | 86.3183（约 1.951×） |

第一轮旧路径存在明显离群值，表中保留全部原始数据且中位数不使用它。该结果只表示 GDI 缓冲创建/复用，不代表完整界面帧耗时。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-desktop --release window_paint_ '--' '--nocapture' '--include-ignored'

## SPU 按钮分组有序回溯（2026-11-08，未提交工作区）

`dvda-menu/src/spu/image.rs` 将 `groups^buttons` 全组合扫描改为逆序回溯：最后按钮的组号变化最慢、第一个按钮变化最快，与旧组合编号顺序一致；完整组合仍按原按钮顺序重建 palette，并共用同一个 `groupmap` 验收函数。冲突分支提前跳过，不设置改变行为的组合上限。

普通测试覆盖空按钮、useimg 开关、首个成功组合、布尔结果、最终 `groupmap`、`numgroups` 和成功路径逐按钮组号；全不透明 palette 也覆盖最终验收路径。完整 dvda-menu 的 37 项测试通过。

Release 使用 12 个按钮、每种实现 20 次调用、旧/新顺序交替六轮：

| 组数 | 轮次 | 全组合扫描 ms | 有序回溯 ms |
| --- | --- | ---: | ---: |
| 2 | 1 | 4.4422 | 0.0773 |
| 2 | 2 | 2.7653 | 0.0572 |
| 2 | 3 | 5.3442 | 0.0671 |
| 2 | 4 | 2.3891 | 0.0617 |
| 2 | 5 | 4.3242 | 0.0632 |
| 2 | 6 | 2.4800 | 0.0575 |
| 2 | 中位数 | 3.5448 | 0.0625 |
| 3 | 1 | 8.1835 | 0.0468 |
| 3 | 2 | 9.9807 | 0.0445 |
| 3 | 3 | 6.8711 | 0.0312 |
| 3 | 4 | 7.7433 | 0.0264 |
| 3 | 5 | 8.3521 | 0.0295 |
| 3 | 6 | 6.0354 | 0.0282 |
| 3 | 中位数 | 7.9634 | 0.0304 |

两组为无解搜索，三组为成功搜索，两种路径均明确提速。结果只表示该确定性 SPU 分组基准，不代表完整菜单制作耗时。

复现命令：

    & tools\win-build\cargo-msvc.ps1 test --manifest-path rust/Cargo.toml -p dvda-menu --release spu_backtracking_throughput '--' '--ignored' '--nocapture'

## owner-draw 控件位图缓存试验（2026-11-08，未保留）

以父窗口、控件句柄和尺寸为键缓存最多 32 个 owner-draw DIB，并在父窗口销毁时统一释放。真实按钮的像素与释放测试通过，但 Release 每个实现 100 次调用、交替六轮没有明确收益：

| 轮次 | 每次创建/销毁 ms | 控件缓存 ms |
| --- | ---: | ---: |
| 1 | 8.6989 | 8.4706 |
| 2 | 8.1842 | 7.6554 |
| 3 | 7.5480 | 7.1040 |
| 4 | 8.7388 | 7.8142 |
| 5 | 7.9252 | 8.4828 |
| 6 | 8.2446 | 9.2330 |
| 中位数 | 8.2144 | 8.1424 |

收益处于噪声范围且后两轮更慢，未达到保留标准。实现、基准和清理改动已全部撤销；主窗口单张 DIB 缓存不受影响。

## MLP 有符号量化移位试验（2026-11-08，未保留）

将 `value / 2^qss` 改为等价的向零截断移位公式，并验证 qss=0..15、边界值和 10,000 个确定性样本。为避免编译器消除重复纯计算，两个 const 变体均使用 `#[inline(never)]` 和 `black_box`。最终六轮结果：

| 轮次 | 原除法 ms | 移位公式 ms |
| --- | ---: | ---: |
| 1 | 0.3456 | 0.2869 |
| 2 | 0.2838 | 0.2847 |
| 3 | 0.2902 | 0.3010 |
| 4 | 0.2785 | 0.2773 |
| 5 | 0.2939 | 0.2946 |
| 6 | 1.2751 | 0.3077 |

中位数约 0.2895 vs 0.2915 ms，基本持平，未达到保留标准。实现和基准已撤销，MLP 输出路径不因此改变。

## 后续候选（静态审计，未测收益）

1. `dvda-core/src/preparation/state.rs`：逐条 `value.clone()` 可评估借用反序列化，收益预计较小，需以真实状态加载基准决定。

## 未运行项目

- `release_files_benchmark`：缺少真实输入文件与报告环境，NOT RUN。
- `synthetic_batch_worker_benchmark`：缺少冻结 oracle 和指定打包 EXE，NOT RUN。
- `real_cached_mlp_preflight_benchmark_is_read_only`：缺少完整真实缓存环境，NOT RUN。
- ARM64：当前 x64 会话没有原生 ARM64 执行验收，NOT RUN。

这些项目不计为通过，也不提供估算耗时。完整制盘、真实大封面、外部 oracle 和播放器兼容性不能由上述微基准确认。
