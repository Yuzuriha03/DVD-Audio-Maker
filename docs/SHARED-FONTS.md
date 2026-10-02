# 共享字体数据，减少发布包体积

[简体中文](SHARED-FONTS.md) | [English](SHARED-FONTS.en.md)

后续优化：[原生工具精简记录](NATIVE-SLIMMING.md)，发布包进一步降至 76.98 MiB。本页保留字体共享阶段的测量结果。

日期：2026-10-02。仍为 Windows x64、GUI-only、需要另装 .NET 10 Desktop Runtime 的精简包。沿用原来的 ZIP 格式和 `CompressionLevel.SmallestSize`，不增加用户端下载或解压步骤。

## 实测体积

| 项目 | v1.0 发布包 | 共享字体后 | 减少 |
|---|---:|---:|---:|
| ZIP | 117.61 MiB | 92.30 MiB | 25.31 MiB / 21.52% |
| 解压后 | 223.83 MiB | 193.35 MiB | 30.48 MiB / 13.62% |
| 字体文件 | 47.05 MiB | 16.57 MiB | 30.49 MiB |

ZIP 精确大小：123,318,008 → 96,780,971 字节。新包位于 `build/font-shared-x64-final/DVD-Audio-Maker.zip`。

## 实现

原先的 SC、JP、KR 三份 OTF 重复保存了相同的字形轮廓、字宽等 OpenType 表。打包器将相同表共享存储到标准的 `DvdaNotoCJK-Regular.ttc` 中，保留三个区域 face 的全部字体表。每张表的内容保持原样；未做字符子集裁剪。

`type-dvda-cjk.xml` 用明确的 face 索引注册 `DVDA-Noto-Sans-CJK-SC`、`DVDA-Noto-Sans-CJK-JP` 和 `DVDA-Noto-Sans-CJK-KR`。程序按名称选择区域字形，直接读取集合；不需要首次启动再展开为三份 OTF。

GUI 默认设置与可选的开发 CLI 启动脚本使用这些名称。旧配置中指向当前安装目录内原有随包 OTF 的路径会自动迁移，用户指定的其他自定义字体设置保留。预编译输入仍可提供三份原有 OTF，也可复用新包中的 TTC；原始输入目录不会被改写。

所有原生程序、动态库、MLP 编码核心及菜单素材保持原样。更新了字体文件清单和打包说明。

## 验证范围

- 107 项兼容性测试通过，新增字体共享、区域 face 顺序、往返提取、确定性、错误区域拒绝及自定义字体保留断言。
- 三个区域的每张原始字体表均逐字节保留；18 pt 与 36 pt 的中日韩混合文字渲染像素相同。
- 检查了带空格路径的包搬迁、未设置全局字体环境变量时的字体发现，以及中英文 GUI 启动。
- 48 kHz / 16-bit / 单声道、48 kHz / 24-bit / 双声道、96 kHz / 24-bit / 六声道的完整 MLP 与既有基准逐字节相同。
- 中日韩三个专辑、六首音轨的完整制盘与成品校验通过；六个 MLP 与旧包逐字节相同，42 张菜单、按钮和静图的像素相同。封面分别采用 JPG、PNG、WebP。
- ZIP 内容与 SHA-256 清单逐项一致；格式与压缩设置未改变。

## 对照环境

旧版字体检查会拒绝含空格的字体文件路径，原生菜单工具也存在程序路径重复引号问题。制盘对照使用 Windows 8.3 路径指向同一套原始文件，未改写旧包。另测得原生 ImageMagick 在非 ASCII 安装路径下读取字体的既有问题，原 OTF 与新 TTC 均受影响；本次没有改动第三方程序。

制盘样本使用 ASCII 卷标、每首 4 秒的音轨、每页一首的专辑菜单和静图，索引阈值设为 99。旧包在仅一个索引页时出现 `MENU_INDEX_ARROW_MISSING`，因此本次完整制盘对照不包含索引页；不将这部分计作实机通过。

完整记录见 [shared-font-validation.json](shared-font-validation.json)。最终 ZIP SHA-256：`2cdad133addfe64ef4527a829730597cbf6dbe7949d3e155ee059ed7cf643901`。

本次生成本地候选包，没有改动已发布的 v1.0 标签和 Release 附件。
