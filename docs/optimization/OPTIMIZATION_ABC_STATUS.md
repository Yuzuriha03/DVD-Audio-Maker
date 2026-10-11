# A/B/C 优化状态（MSVC）

本文件记录已经通过代码审查和本地测试的优化范围。性能数字只在有可重复基准时记录；没有把理论估算当成实测结果。

## A：音频并行

已完成可复用的 `process_audio_jobs` 批处理 API。它使用 Rayon，在输入数量达到阈值时并行执行纯 per-track 工作，并保持结果顺序。GUI/日志回调由调用方在主线程更新，因此不会把可变回调状态或媒体 FFI 句柄跨线程共享。

## B：MLP 批处理

已完成可复用的 `encode_mlp_groups` API。它只并行化独立编码任务，结果保持输入顺序；暂不自动接管 `mlp_workflow` 的 staging、缓存发布和进度状态，这些状态仍由工作流串行提交。

## C：流水线

`Pipeline::execute` 已实现为有界多阶段流水线：每个阶段运行在独立线程，阶段间使用 `sync_channel`，最终按输入序号恢复顺序。禁用优化时走同一语义的串行路径，空输入和无阶段输入直接返回。

## 验证边界

- `cargo test --manifest-path rust/Cargo.toml -p dvda-core pipeline_optimization --lib`：通过。
- 音频和 MLP API 具备单元测试，覆盖顺序保持。
- 默认 direct-bridges 构建需要 MSVC 版本的 FFmpeg、ImageMagick 和菜单 vendor 输入；没有这些输入时构建脚本必须明确失败。
- 尚未宣称真实媒体批处理的加速百分比；需要在固定 MSVC 依赖和固定素材集上单独采集基线与优化后数据。
