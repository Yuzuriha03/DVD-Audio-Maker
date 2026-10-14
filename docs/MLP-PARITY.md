# external MLP encoder 码流对照

2026-10-14 的验收约定：比较 MLP 编码码流的 SHA256，不以解码后的 PCM 相同代替。机器和时间相关的辅助元数据可以在比较时排除，生成的 MLP 文件本身不得修补。

## 默认元数据

DVD-Audio-Maker 的生产编码入口默认生成一个 `0x4000` 辅助 TLV 包：

- `0x1000`：`DVD-Audio-Maker Rust MLP`，标识实际编码实现。
- `0x1100`：计算机名经应用域前缀和 SHA256 处理后的摘要。它不是硬件序列号或授权码；修改计算机名会改变此摘要。
- `0x1400`：`unix-utc:<秒数>`，记录本次编码开始时的 UTC Unix 时间，不随编码过程每秒刷新。

这些数据通过现有 MLP 辅助位序列写入，每个访问单元一位，不新增音频载荷或改变编码决策。显式传入 `MSCTX001`/`MSCTX002` 上下文时原样使用该上下文，不再自动插入默认字段。旧参考向量通过显式空 TLV 保留原始文件哈希。

## 两种 SHA256

`tools/win-build/compare-mlp-coded.py` 同时报告原始文件哈希 `file_sha256` 和 `coded_sha256`，二者不可混称。

比较器逐个读取 MLP 访问单元，检查长度、主同步头、子流数和原始头部奇偶校验，然后仅在内存中：

1. 清除第一个子流目录的 bit 12，即辅助标识流位。
2. 按该位原值翻转访问单元头部中对应的一个奇偶校验位。
3. 将剩余完整字节送入 SHA256。

MLP 音频载荷、预测参数、残差、音频校验数据、AU 长度、到达时间戳、其他目录位及主同步头全部保留。不解码 PCM，也不写回任何输入文件。头部校验损坏会报错，不会在正规化过程中被修复。未知的额外目录字布局会被拒绝。

**排除范围是整个辅助标识 TLV 流**，其中 MLP 还含版本、操作系统和授权标识，并非只删除两个时间/机器字段。`coded_equal` 只证明该明确范围以外的 MLP 码流一致，不代表原始文件 SHA256 相同。

```powershell
python tools/win-build/compare-mlp-coded.py original.mlp candidate.mlp --report comparison.json
python tools/win-build/test-compare-mlp-coded.py
```

负例测试验证：修改音频载荷、AU 到达时间或第二子流目录位会比较失败；破坏奇偶校验、截断或空文件会被拒绝。

## 音乐样本流程

本次验收从音源目录随机抽取完整 FLAC，以独立 FFmpeg 按原采样率、位深生成相同 PCM，再通过原版 MLP 正常编码，并调用产品的 `encoder::execute_rust` 路径进行比较。记录输入、工具和输出的 SHA256，分别比较完整文件和排除辅助标识流后的码流。

额外使用相同显式元数据验证完整文件逐字节一致。生产默认编码不读取原版文件。临时抽样脚本和开发入口已删除；保留码流比较器及负例测试。

三首真实曲目的通过不能推广为所有声道分组、下混和全部输入都已与原版一致；需要按各自配置继续扩大覆盖。

## 2026-10-14 实测

从 `D:\yyz57\Music\completed\鸣潮先约电台` 的 147 首 FLAC 中抽取三首，随机种子为 `5776984983609059872`。原版参考通过 `E:\yyz57\Downloads\MLP\external MLP encoder\external-mlp-encoder.exe` 正常编码得到。

| 曲目 | 格式 | MLP 字节数 | 排除辅助标识流后的 SHA256（两边相同） |
| --- | --- | ---: | --- |
| Waking of a World (feat. Dyon Joo) [Korean Version] | 44.1 kHz / 24-bit / 2ch | 46,798,364 | `61c06352129f73aa134914ed9f2886db62e1cebb99944b59663f3ab1cedb45c0` |
| 未完成の歌 | 48 kHz / 24-bit / 2ch | 60,742,574 | `8fc388d3bbcdbacddb6ac88796b431778e2267d49814145b96a84105522ee625` |
| 切り咲く | 48 kHz / 24-bit / 2ch | 52,524,768 | `5c5107a74f6a7c9d60d624b0d551972dd87d8dede506f2d251ee99a2f7f3ff92` |

三首默认 Rust 输出均实际包含 `1000/1100/1400` 元数据。默认原始文件哈希与 MLP 不同；排除辅助标识流后全部一致。额外使用相同显式元数据时，三首完整文件均逐字节一致。首首原版重复编码本身的文件哈希也不同。

原版捕获记录：`build/MLP-music-20261014/report.json`。
加入 Rust 身份/时间戳后的完整报告：`build/MLP-music-stamped-20261014/report.json`。
音源解码 MD5 只用于确认输入未变化；输出验收没有解码或比较 PCM。
