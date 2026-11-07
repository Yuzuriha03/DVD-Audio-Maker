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

## 未运行项目

- `release_files_benchmark`：缺少真实输入文件与报告环境，NOT RUN。
- `synthetic_batch_worker_benchmark`：缺少冻结 oracle 和指定打包 EXE，NOT RUN。
- `real_cached_mlp_preflight_benchmark_is_read_only`：缺少完整真实缓存环境，NOT RUN。
- ARM64：当前 x64 会话没有原生 ARM64 执行验收，NOT RUN。

这些项目不计为通过，也不提供估算耗时。完整制盘、真实大封面、外部 oracle 和播放器兼容性不能由上述微基准确认。
