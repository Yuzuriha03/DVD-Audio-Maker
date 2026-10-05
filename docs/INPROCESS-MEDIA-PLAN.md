# 进程内媒体处理方案

方案已经落地：Rust GUI 通过 `dvda-media.dll` 处理探测、解码、PCM/FLAC 输出、ALAC 检查和成品校验；不启动外部 FFmpeg/FFprobe。必要 FFmpeg 库由源码构建并随包提供，发布包不需要用户安装 FFmpeg。

配置只使用 JSON profile。`config.env`、旧 FFmpeg/FFprobe 路径设置和外部程序回退已经删除。开发阶段如需对照，只能通过显式测试环境变量选择已验证的原生目录。

媒体 DLL 与 MLP、图像和菜单组件分开构建，缓存键包含库身份。失败或取消的输出不会发布，MLP 文件不会在编码后修补。

验收命令和组件目录见 [Windows 构建说明](../tools/win-build/README.md) 以及 [进程内媒体处理](INPROCESS-MEDIA.md)。
