# Update log

## v1.1

### 中文

- 新增繁体中文与韩语界面、核心消息、CLI 语言选择及系统语言检测支持。
- 对齐各语言 DVD-Audio 菜单固定文案和 SC／TC／JP／KR 默认地区字形；保留用户路径、曲名与专辑名的 Unicode 内容。
- 增加无损菜单字体集合优化工具，保留全部字符及四地区字形，并校验轮廓、布局与度量。
- 修复原生 MLP 编码冻结基准回归：将主预测器扰动搜索从 4 次恢复为历史 C 实现的 16 次；116 个冻结用例的长度与原始 SHA256 全部通过。
- 保持发布包 GUI-only 的分发方式。

验收说明：经用户授权，本次及以后以经独立 MLP 解码后的完整 PCM 样本数据与编码输入的 SHA256 一致作为无损验收标准，统一样本字节表示并排除容器头。允许编码码流和压缩大小不同，不再要求完整旧 C 严格对照作为发布门槛。格式兼容性检查继续保留；116 个冻结向量及编码哈希用于回归诊断，不应表述为全部输入与旧 C 字节一致。

### English

- Add Traditional Chinese and Korean support across the GUI, core messages, CLI language selection and system-language detection.
- Align localized DVD-Audio menu labels and default SC/TC/JP/KR regional glyphs while preserving Unicode user paths and metadata.
- Add lossless menu-font collection optimization with character, regional-glyph, outline, layout and metric preservation checks.
- Fix the native MLP frozen-vector regression by restoring the primary predictor search from 4 to the historical C budget of 16 perturbations. All 116 frozen cases pass length and raw SHA256 checks.
- Retain GUI-only distribution packaging.

Acceptance note: by user authorization, lossless acceptance for this and future releases requires matching SHA256 of the complete independently MLP-decoded PCM and encoder input, using the same sample representation and excluding container headers. Encoded bytes and compressed sizes may differ; complete historical C comparison is not a release gate. Format compatibility checks remain applicable. The 116 frozen vectors and encoded hashes are regression diagnostics, not a claim of historical C byte equality for all inputs.
