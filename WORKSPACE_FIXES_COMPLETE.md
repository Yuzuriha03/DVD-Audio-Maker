# 当前工作区修复记录

本地用户提交 `b368d11` 把构建目标改为 MSVC，但同时保留了 GNU vendor/脚本，并在缺少 FFmpeg/ImageMagick 时生成空布局。这会让 `cargo check` 看似成功、最终链接失败。修复工作统一采用 MSVC only 策略：

- 所有生产入口和工具链验证使用 `x86_64-pc-windows-msvc`；
- media/image 依赖缺失时直接失败，不生成空 stub；
- menu direct-link 缺少 MSVC vendor 归档时直接失败；
- 删除未完成且与 `resources.rs` 重复的 session bridge 实验导出；
- 用真实的有界多阶段 `Pipeline::execute` 替换空实现；
- 音频和 MLP 并行模块提供保持顺序的可测试批处理 API。

验证分为两层：无外部依赖的 workspace/core 测试，以及待 MSVC 原生依赖重新构建后的 direct-bridges/media/image 集成验收。发布包仍代表旧提交，修复完成后需重新构建再发布。
