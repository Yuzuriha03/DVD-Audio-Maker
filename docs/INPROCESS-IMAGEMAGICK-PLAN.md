# ImageMagick 进程内迁移方案与调用范围核查

日期：2026-10-02。状态：已完成两层迁移、原生依赖裁剪、完整制盘回归和精简包验收。之前的已验证媒体包仍保留。

## 实施前的外部 FFmpeg 调用范围

当前本地发布包是 build/inprocess-media-x64-release/DVD-Audio-Maker。

正常 GUI 支持的工作流，包括音源信息读取、解码检查、ALAC 转 FLAC、PCM 准备、MLP 解码比对、菜单首帧提取及成品校验，均使用进程内媒体库。ProjectSettings.ToOptions 固定内置转换/探测入口，ProcessRunner 在创建进程前分派到 BuiltinMedia。MLP 编码核心是另一份固定哈希的进程内 DLL。dvda-author 的 MLP 处理直接使用 libavcodec API。

之前已完成实际验证：PATH 限制为 Windows/.NET，并将导入配置中的 FFmpeg/FFprobe 路径设为不存在文件；GUI 完成完整 ISO 制作和校验。见 inprocess-media-validation.json 的 release_regression 和 final_location_smoke。

不能把这个结论扩展成“整个代码仓库任何入口都禁止启动 FFmpeg”：

1. 开发 CLI 保留显式外部转换器路径；集成测试用外部 FFmpeg/FFprobe 生成参考音频并独立验证结果。
2. ImageMagick 的随包 delegates.xml 仍有 mpeg:decode / mpeg:encode 对 ffmpeg.exe 的通用委托。当前菜单只向 ImageMagick 提交图片，视频抽帧由内置媒体库完成，正常流程不使用这些委托；但这个潜在调用路径尚未从 ImageMagick 配置中移除。
3. 上述结论只针对本次本地发布包，不代表历史发布包已同步替换。

## 实施前的 ImageMagick 调用链

第一层是 C#：

- MenuAssetBuilder：背景缩放、变暗、画布填充、播放静图、透明叠加图、尺寸检查。
- MenuFontResolver：字体枚举及中日韩墨迹探测。
- MenuBuildVerifier：菜单叠加图统计。
- MenuVisualVerifier / VerificationPipeline：菜单帧裁切、图像栈、均值/最大值/标准差等检查。

第二层是原生 dvda-author：menu.c 通过 system(win32quote(...)) 调用 mogrify / convert 绘制文字、按钮高亮、选中层与专辑索引；command_line_parsing.c 也有空白背景转换调用。

目前 convert.exe / mogrify.exe 是小型转发入口，最终仍创建 magick.exe 进程。仅将 GUI 改成 DLL 不足以消除整个制盘流程中的 ImageMagick 子进程。

核查的原生来源：D:/dev/winbuild/src/src/menu.c、command_line_parsing.c 和 mlp.c。该目录已有本地改动；迁移应在独立构建副本中完成，保存可审阅补丁，不覆盖这份工作树。

## 实施路线

1. 准备按需编译的 Windows x64 ImageMagick 动态库和小型 C ABI。C# 通过 P/Invoke 调用，原生 dvda-author 也调用同一个图像库。保留现有 Q16/HDRI 精度、中日韩字体 face、透明通道和色彩行为；以实测像素对照为准，不假设不同版本或构建会自动等价。
2. 将第一层已有图片请求移入库接口，完整覆盖图像栈、字体列表、文本渲染、identify/统计、读取 JPG/PNG/WebP、写入 PNG/JPEG、输出进度与错误。不要仅覆盖背景缩放而遗漏校验及字体预检。
3. 为第二层制作 dvda-author 源码补丁并重新编译，把所有 ImageMagick 专用 system 调用改为库调用；其余菜单编码器、复用器等流程维持现有职责。不能用一个新 EXE 再包一层 magick.exe 来宣称迁移完成。
4. 在打包和工具检查中去除 magick.exe、convert.exe、mogrify.exe、identify.exe 的要求。保留必要字体与配置，禁用外部视频及其他不需要的 delegates；不依赖 PATH 上的 ImageMagick 或 FFmpeg。
5. 保留取消、超时、错误信息和失败输出清理。图像库的全局配置/日志状态必须明确管理，避免多任务互相修改工作目录、环境或输出；验证线程安全和取消粒度。
6. 通过完整回归后再产出新的 GUI-only、x64、framework-dependent 包。记录 DLL 闭包、哈希、真实 ZIP/解压体积，保持现有 ZIP 格式及压缩参数。

本机 ImageMagick 源码头文件已提供 MagickWandGenesis、MagickCommandGenesis、MagickImageCommand 和进度回调接口，可作为桥接实现的基础。现存源码版本 7.0.10-35 与当前 EXE 的 7.0.8-47 不同，不能把它们视为已经验证的可互换构建。

## 完成标准

- GUI 与原生制盘程序均无 ImageMagick EXE 调用；完整制作/校验时监测进程启动以验证，不只依靠日志里不出现程序名。
- 在缺少外部 FFmpeg/ImageMagick、禁用委托的隔离环境下完成：检查、预演、制作、菜单/静图和成品校验。
- 普通专辑页、索引页、翻页箭头、按钮三态、中日韩文字和透明层均有实际样本；比较尺寸、解码像素、按钮 XML 和校验结果。已有索引菜单问题需单独记录，不以关闭索引代表验证了索引。
- JPG/PNG/WebP 输入、Unicode/空格路径、字体区域选择、失败与取消可回归。
- MLP 核心保持固定哈希，音频/MLP 回归仍逐字节一致，不做编码后修补。
- 新包是否更小以实际测量为准。当前 magick.exe 为 6,847,032 字节，ZIP 中占 6,444,401 字节；两个转发入口各 5,632 字节。仅换调用形式并不自动节约这些图像算法和字体依赖。

## 实施记录

已按以上路线迁移 GUI 与原生制盘两层，并改为精简 x64 图像 DLL。外部委托、动态 coder 模块及旧命令行入口均已移除。能力、测试结果和像素差异见 [内置图像处理](INPROCESS-IMAGES.md)。本文的调用范围核查描述迁移前的基线，不代表新包仍有这些外部调用。
