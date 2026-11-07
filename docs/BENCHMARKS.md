# P0 性能基线报告

tools/win-build/benchmark-report.py 复用 Rust 测试套件中已有的 ignored 基准，不修改生产实现。默认在 x64 上对每个基准独立执行三轮，生成 build/benchmarks/baseline-x64.json 和逐项日志。

## 运行

    .venv\Scripts\python.exe tools\win-build\benchmark-report.py --output build\benchmarks\baseline-x64.json

脚本要求 --rounds 3，发现、编译和测试命令沿用现有 cargo-msvc.ps1 / Cargo JSON 产物。任一测试返回非零时报告标记为 FAIL，脚本也返回非零；失败不会被写成通过。

## 报告内容

- version=1、UTC 时间、Git 提交、目标架构和发现命令；
- 每项基准的确定性输入说明、生成器源文件 SHA-256、输入数据 SHA-256；
- 三轮耗时、单轮样本、每轮日志、进程峰值工作集、临时目录采样峰值；
- 三轮中位耗时和独立计算的期望输出 SHA-256；
- 实际出现的 [METRIC] 事件及 worker 峰值；测试未产生该数据时字段为 null 并附原因。

PCM 测试会在内部验证输出样本，报告中的期望输入/输出哈希由脚本按同一确定性生成规则独立计算。进度基准使用规范化的任务数、消息数和最终标记作为输出证据。

## 测量边界

- 工作集是每个测试进程的峰值，不是整个工作流或系统的内存峰值。
- 临时空间只按已知测试目录每 50 ms 采样，短生命周期文件可能漏采，不能视为精确磁盘峰值。
- 没有 [METRIC] 的测试不会虚构 worker 峰值。
- release_files_benchmark、真实缓存预检和合成端到端测试需要真实文件或冻结 oracle，缺失时记录为 NOT RUN。
- ARM64 在当前 x64 会话不能进行原生执行验收，记录为 NOT RUN，不计为通过。
- 报告只表示这些固定微基准，不代表完整专辑制作速度或播放器兼容性。
