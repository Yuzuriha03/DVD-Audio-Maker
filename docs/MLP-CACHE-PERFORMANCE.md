# MLP 编码前检查优化

## 原因与改动

原流程输出 `MLP-PROGRESS 0/147` 后，串行检查所有已有 MLP：源文件与
输出指纹、完整访问单元/CRC/奇偶校验，再读取源文件和 MLP 音频参数。
循环内部没有进度记录，全部命中缓存时直接跳到 `147/147`。
2026-10-07 的原始运行日志中，这一阶段耗时 145 秒。

现在：

- 编码前显示独立的“正在检查已有 MLP”状态，完成每首即更新 `x/n`、曲名
  和进度条；检查结束明确列出可复用数量及需要编码数量。中、英、日三语言一致。
- 自动按核心数分配最多 16 个检查线程，任务完成后立即领取下一首，不按批次等待。
  与 MLP 编码使用同样的上限；实际线程数也不超过当前曲目数量。
- 检查结果按原曲目索引回填，待编码曲目和缺失音源诊断按原顺序输出。
- 编码缓存新增 `parameters_version`、`source_parameters` 和
  `output_parameters`。仅在原有源/输出指纹、编码组件身份、目标参数及
  **完整 MLP 扫描**通过后复用音频参数；未知版本、缺失/无效参数重新读取。
  旧格式缓存命中时补齐参数证据，不要求重编码。
- C17 CRC-8/CRC-16 使用不可变查找表，每帧不再重新生成查找表。
- 文件扫描复用最大访问单元大小的栈缓冲区及文件读取缓冲，不再逐帧 malloc/free。
- 复用扫描返回的文件长度，避免额外 stat；修复短读错误路径中关闭文件后
  调用 `ferror` 的问题。

取消与异常由协调线程统一处理：停止领取新任务，同时继续排空有界事件队列，
避免工作线程在发送消息时阻塞。已经进入 C 完整扫描的单个文件会完成当前
扫描再退出；尚未添加 C 层逐帧取消 ABI。取消/错误时不保存本轮参数缓存。

本次不修改 MLP 编码算法或编码文件，不通过跳过 CRC 校验获得提速。
正常的编码组件身份变更仍会使缓存失效；本次没有绕过该规则。

## 实测

本机 147 首已有 MLP，总计 7,614,869,370 字节：

| 测试 | 耗时 | 说明 |
| --- | ---: | --- |
| 原日志中的编码前检查 | 145 秒 | 整个串行阶段，原运行 |
| 旧 C DLL 完整扫描 | 121.437 秒 | 同文件先预热，再单线程扫描 |
| 新 C DLL 完整扫描 | 26.010 秒 | 与旧 DLL 同 ABI 结果，约 4.67 倍加速 |
| 新流程，4 路首次检查 | 10.131 秒 | 旧缓存无参数证据，需读取参数 |
| 新流程，4 路复用参数 | 9.995 秒 | 147 次曲目进度更新 |
| 新流程，1 路复用参数 | 41.699 秒 | 用于检查并行收益 |
| 16 路上限的自动检查，首次 | 4.469 秒 | 本机检测到 8 核，因此实际 8 路 |
| 16 路上限的自动检查，复用参数 | 4.119 秒 | 实际 8 路，147 次曲目进度更新 |

完整流程计时与独立 C 计时为不同测试；磁盘缓存和后台负载不同，不直接
相减估算探测成本，也不保证其他机器固定耗时。真实样本流程测试只读文件
和实际缓存，参数升级只保存在测试内存中，未运行制盘或编码。
上表 4 路计时是初版基准；随后依用户要求将自动并行上限调整为 16 路，
不能将 4 路结果表述成 16 路结果。
当前自动模式：`min(可用核心数, 16, 曲目数)`。本机自动模式实际为 8 路，
不人为启动超过检测结果的 16 个线程。

## 回归验证与复现

- 全部 147 首完整扫描的返回状态和 ABI 结构与旧 DLL 相同；8 首大小混合
  样本的 alignment 输出与旧 DLL 逐字节一致，且与输入相同。
- 144 项 CRC 独立多项式计算对照及无效输入边界。
- C17 历史 CRC、PTS、文件/缓冲区、损坏输入、PCM 比较回归。
- 116 项冻结编码整文件 SHA-256/长度与解码 PCM 对照；批量编码 168 份
  输出继续匹配冻结样本。
- 新增旧缓存升级/参数复用、重采样与位深标记保持、曲序与进度单调性、
  保持文件大小/修改时间/头尾指纹的中部损坏检出、取消、异常队列排空、
  缺失音源属性及检查线程上限测试。
- GUI 三语言摘要与现有工作区测试通过。

构建新 DLL 并与旧 DLL 对照（报告放 D 盘临时构建目录）：

```powershell
python tools/win-build/build-formats-runtime.py --msys-root C:/msys64 --output D:/dvda-check/formats
python tools/win-build/test-formats-optimization.py --before D:/previous/dvda-formats.dll --after D:/dvda-check/formats/dvda-formats.dll --samples D:/samples/mlp --report D:/dvda-check/formats-report.json
$env:DVDA_FORMATS_NATIVE_DIR = 'D:/dvda-check/formats'
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-native -- --include-ignored
cargo test --manifest-path rust/Cargo.toml --target x86_64-pc-windows-gnu --offline -p dvda-core --lib mlp_workflow -- --include-ignored --skip real_cached --skip another_volume
```

真实缓存检查计时使用忽略测试
`real_cached_mlp_preflight_benchmark_is_read_only`，设置：

- `DVDA_MLP_PREFLIGHT_MANIFEST`：音源清单。
- `DVDA_MLP_PREFLIGHT_ROOT`：音源根目录。
- `DVDA_MLP_PREFLIGHT_OUTPUT`：已有输出及 `mlp-cache.json` 所在目录。
- `DVDA_MLP_PREFLIGHT_MEDIA` / `DVDA_MLP_PREFLIGHT_ENCODER`：与该缓存
  身份一致的媒体/编码 DLL。
- `DVDA_MLP_PREFLIGHT_REPORT`：计时报告输出文件。

任何缓存未命中都会使测试失败，不会悄悄编码或覆盖真实样本。
本次未重跑 147 首完整制盘流程；正式发布需重新打包 GUI 与新 C DLL。
