# 性能优化规划备忘（历史，未验证）

> **历史文档（2024 年规划稿）。** 本文不是实测报告，也从未通过固定输入的前后基准验证；其中的百分比均为当时的“预期收益”，不得作为性能声明引用。当前可复现基线与测量边界见 [docs/PERFORMANCE_VERIFICATION.md](../../docs/PERFORMANCE_VERIFICATION.md) 与 [docs/BENCHMARKS.md](../../docs/BENCHMARKS.md)。

## 当时记录（均已过时）

- 计划中的 `target-cpu=native`、`rayon` 依赖与 `gui-release.cmd`、`cli.cmd encode` 等命令从未作为当前构建/使用方式保留；仓库当前不强制 `target-cpu=native`，并行使用有界 std 线程池，不使用 rayon。
- 文中“整体 10-15%”“MLP 20-30%”“综合 30-40%”等数字没有基线提交、固定输入或中位数测量，属于未验证预期，已从本备忘删除，避免被误用为实测结论。

## 当前做法

- 需要性能结论时，先用固定输入运行 `tools/win-build/benchmark-report.py`，保留 baseline 提交、输入/输出 SHA-256 与三轮中位数，再与同一输入的改动后结果比较；微基准不得外推为整盘制作收益。
- 当前门禁与复现说明见 [开发说明](../../docs/DEVELOPMENT.md) 与 [P0 性能基线报告](../../docs/BENCHMARKS.md)。
