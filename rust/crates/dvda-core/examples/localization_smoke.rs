fn main() {
    let samples = [
        "发现 1 个音频文件 (FLAC/M4A)",
        "共需重采样 0 首",
        "总计 1 首",
        "正在制作第 1 张光盘。",
        "任务完成。",
        "LPCM 音频码率超过 9.6 Mb/s，请降低目标参数或选择 MLP 编码。",
    ];
    for language in ["zh-CN", "zh-TW", "en", "ja", "ko"] {
        println!("--- {language}");
        for sample in samples {
            println!("{}", dvda_core::localization::translate(language, sample));
        }
    }
}
