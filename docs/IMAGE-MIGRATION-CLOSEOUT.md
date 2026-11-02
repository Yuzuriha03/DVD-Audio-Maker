# Rust 图像迁移收尾报告

## 结论

独立构建、单 EXE 制作/校验、体积与性能实测以及本轮许可证核对已完成。**整体迁移终验尚未完成，不作为发布批准。** 真实播放器交互与集中人工视觉验收仍待执行；ARM64 未在真实设备验收。本轮未修改 MLP 算法，未提交、推送或发布。

## 可复核证据

本地原始证据位于 `build/rust-image-closeout/`（构建输出，不应假定 Git 克隆中存在）：

- `package-final.log`：独立 target、纯 Rust author、静态 FFmpeg 重打包成功。
- `final-onefile-pal.json`、`final-onefile-ntsc.json`：最终 EXE 在仅系统 PATH、无生产 ImageMagick 的独立环境中通过 MLP/LPCM 制作和校验，包含菜单及静图；这不是实际播放器播放测试。
- `image-author-tests.log`：dvda-image/dvda-author 合计 74 项通过。
- `visual-fixtures.log`：direct-bridges 配置下 PAL/NTSC menu_and_title_boundaries 两项通过；保留产物在 `visual/`。先前未启用 direct-bridges 的尝试出现 DLL 126，属于测试配置错误；修正后通过。未加 --ignored 的空运行不计通过。
- Functional 重试：Failed executables 0；显式未运行 13 项，见下文。
- 最终 `git diff --check` 通过；存在 Git 换行提示，无空白错误。

## 最终候选产物

位置：`build/rust-image-closeout/release/`。

| 产物 | SHA256 |
|---|---|
| 新 EXE | `3e575e4410dfd88e45b708293c9c2ace10d748d910db02bae2c1d9a6fe2ff5dd` |
| 新 ZIP | `f037c7515a336bc559c5f8e420b0ed217be7d0e7422188f8f4178bb5a2a93322` |
| 冻结旧 EXE | `af66721117fce2f555fd762ad75dd15f360d33d120ecf443aa02b8c1372cdaed` |

## 体积

| 项目 | 旧（bytes） | 新（bytes） | 变化 |
|---|---:|---:|---:|
| EXE | 23,075,328 | 21,621,248 | -6.30% |
| ZIP | 16,694,090 | 16,339,714 | -2.12% |
| 发布目录文件总和 | 23,155,456 | 22,266,784 | -3.84% |

新版已包含补全后的图像许可证。运行时文件计数 10 → 8；两者 runtime_dlls 均为空、均不分发 author 子进程。计数不能替代逐文件来源核对；本轮尚未完成新旧内嵌字体/素材逐文件哈希对照。

## 性能与内存

同机 PAL，第 2、3、5 轮；排除并行编译污染的首轮。单位秒：

| 操作 | 旧中位数 [min,max] | 新中位数 [min,max] |
|---|---|---|
| MLP 制作 | 2.298 [2.094,3.065] | 1.249 [1.175,1.271] |
| MLP 校验 | 1.338 [1.147,1.511] | 0.651 [0.617,0.761] |
| LPCM 制作 | 2.569 [2.566,3.458] | 1.596 [1.578,1.790] |
| LPCM 校验 | 1.431 [1.305,1.521] | 1.093 [0.978,1.096] |

这是 GUI 端到端任务耗时，不是纯图像算法基准。第 2/3 轮使用补许可证前 EXE，第 5 轮使用最终 EXE；图像实现相同，文件哈希不同。小样本结果不代表所有输入。

`memory.json` 单次配对：旧峰值工作集 54,292,480 bytes，新 71,888,896 bytes，**增加 17,596,416 bytes（32.41%）**。以 20ms 间隔采样 GUI 进程的操作系统累计 PeakWorkingSet64，两个启动进程取最大值，不求和。不是 private bytes、进程树峰值或图像专属内存；可能漏掉退出前瞬间，不能宣称内存优化。

## 文档与许可证

- 已更新独立 target 打包入口、静态媒体依赖和纯 Rust 图像说明；同步 v1.1 包名。
- [NOTICE-Image.txt](../NOTICE-Image.txt) 收录锁定图像依赖许可证，并补入 rustybuzz `scripts/ms-use/COPYING` 的 Microsoft MIT 声明；最终包已包含。
- 递归检查锁定依赖中的 LICENSE/LICENCE/COPYING/NOTICE/COPYRIGHT/AUTHORS。tiff 测试图像 CC0/Public Domain 声明涉及测试素材，不作为产品资源分发。
- 本轮是依赖声明与许可证文件核对，不是全部源文件内嵌版权的法律审计。

## 明确未完成与未运行

1. 人工视觉矩阵：PAL/NTSC、多页 Previous/Next、三态高亮、长文本、中日韩 TTC/fallback、无封面；自动几何/导航检查不能替代主观可读性检查。
2. PowerDVD 8 或兼容 DVD-Audio 播放器真实导航/播放。已检查本机 CyberLink 目录，仅发现 Shared files 辅助程序，未发现 PowerDVD 可执行文件；不等于证明全机绝无播放器。不得把产物生成或 MPEG 头检查当播放通过。
3. ARM64 实机验收：NOT RUN。
4. 新旧内嵌字体/素材逐文件哈希对照及 runtime 文件减少项归因尚未完成。
5. Functional 13 项 NOT RUN：5 项需要显式冻结 MLP oracle；7 项仅在 Benchmarks 模式运行；1 项需要另一可写卷。没有把跳过项算作通过。

### Functional 跳过清单

需要冻结 oracle：
- frozen_mlp_matrix_and_lossless_decode
- encoder_transactions_and_parallel_calls
- batch_matrix_matches_frozen_encoder_bytes
- mlp_acquisition_upgrades_legacy_parameter_cache_without_reencoding_or_track_changes
- batch_cancellation_panic_and_partial_failure

仅 Benchmarks：
- normalization_throughput
- packing_32_throughput
- release_files_benchmark
- pcm_comparison_throughput
- inspection_progress_callback_benchmark
- synthetic_batch_worker_benchmark
- real_cached_mlp_preflight_benchmark_is_read_only

另一可写卷：
- encoded_publication_supports_configured_staging_on_another_volume

## 大封面播放静图回归修复

用户手工制作日志中的 `MENU_STILL_FAILED: image working buffers exceed 256 MiB` 暴露了上述旧候选的回归：完整源图预乘浮点副本，以及播放静图在原始高度下先做 15/16 宽度校正的浮点中间图，使大封面超过工作内存预算。

修复保留 256 MiB 检查及 Lanczos3 算法：预乘改为按需读取；等高度宽度校正按 64 行分块、两侧保留 3 行滤波邻域，避免完整高度的浮点中间图。未修改用户设置，也未跳过失败封面。

验证：
- dvda-image release 测试 34 项通过（含显式外部封面测试）；分块结果与完整图缩放的逐像素比较通过。
- 用户音乐目录的 45 张真实封面全部通过宽度校正及 PAL/NTSC 尺寸缩放。
- dvda-core 完整 `write_still` 路径：45 张真实封面全部完成解码、两阶段缩放、黑边扩展、JPEG 写入及帧尺寸检查。
- 修复后 GUI/ZIP 已重新打包；EXE SHA256：`9630166f3a9c26b966f92dfa50e177b290b58313537aabc49133c41456da4d7a`。
- 2026-11-02，用户手动测试修复后版本，确认“没问题，验收通过”。本次大封面播放静图回归的用户验收已通过。
- 自动化侧尚未重新运行新包的完整单文件 PAL/NTSC 工作流或用户全部 147 音轨工程；45 张封面的完整静图路径已独立验证。用户未逐项说明手测覆盖范围，不据此推定上述所有组合均已覆盖。
- 上文体积/性能及哈希属于修复前候选，不能作为本次修复后的测量结果。

## 验收边界

本次静图回归已获用户手动验收通过。该确认不自动补齐前文未记录的真实播放器/PAL/NTSC 逐项覆盖及资源哈希对照，也不等于提交、推送或发布授权；整体迁移的其余收尾项仍以各项证据为准。
