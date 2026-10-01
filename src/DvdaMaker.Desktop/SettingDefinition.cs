namespace DvdaMaker.Desktop;

internal enum SettingKind { Text, Folder, File, Boolean, Number, Choice, Capacity }
internal sealed record SettingChoice(string Value, string Label)
{
    public override string ToString() => Label;
}
internal sealed record SettingDefinition(string Key, string Label, string Group, SettingKind Kind,
    string Help = "", string[]? Choices = null, decimal Minimum = 0, decimal Maximum = 10000000000, bool Advanced = false)
{
    public string DisplayValue(string value) => Key switch
    {
        "DVDA_MLP_SOURCE" => value switch { "surcode-batch" => "内置无损编码（推荐）", "external" => "导入已有 MLP 文件", "surcode" => "导入旧 SurCode 输出", _ => value },
        "DVDA_MLP_SURCODE_SAMPLE_RATE" => value switch { "44100" => "44.1 kHz", "48000" => "48 kHz", "88200" => "88.2 kHz", "96000" => "96 kHz", "176400" => "176.4 kHz", "192000" => "192 kHz", _ => value },
        "DVDA_MLP_SURCODE_BITS" => value + " 位",
        _ => value,
    };
    public static IReadOnlyList<SettingDefinition> All { get; } =
    [
        new("DVDA_SRC", "音源文件夹", "开始设置", SettingKind.Folder, "选择音乐所在目录，自动包含子文件夹中的 FLAC 和 M4A。"),
        new("DVDA_FINAL_DIR", "成品保存位置", "开始设置", SettingKind.Folder, "制作完成的 ISO 光盘镜像保存在这里。"),
        new("DVDA_TITLE", "光盘名称", "开始设置", SettingKind.Text, "显示在光盘卷标中，例如：My Music。为兼容制盘工具，建议使用英文和数字。"),
        new("DVDA_DISC_BYTES", "光盘容量", "开始设置", SettingKind.Capacity, "普通单层光盘选 DVD5；双层光盘选 DVD9。"),
        new("DVDA_MAX_DISCS", "最多制作几张", "开始设置", SettingKind.Number, "填 0 表示不限张数，曲目会自动分盘。", Maximum: 999),
        new("DVDA_BUILD_DIR", "工作文件夹", "开始设置", SettingKind.Folder, "存放编码缓存和制作中的文件，建议选择空间充足的磁盘。", Advanced: true),
        new("DVDA_ISO_PREFIX", "镜像文件名前缀", "开始设置", SettingKind.Text, "留空时根据光盘名称生成。", Advanced: true),
        new("DVDA_GROUP_TRACK_LIMIT", "每组最多曲目", "开始设置", SettingKind.Number, "通常保留默认值 99。", Minimum: 1, Maximum: 99, Advanced: true),
        new("DVDA_PREPARE_CACHE", "复用检查结果", "开始设置", SettingKind.Boolean, "未改变的音源无需重复检查，可以加快下一次制作。", Advanced: true),
        new("DVDA_RESUME", "继续未完成的制作", "开始设置", SettingKind.Boolean, "重试时复用已完成且校验一致的光盘。", Advanced: true),
        new("DVDA_MLP_SOURCE", "音频编码方式", "音频编码", SettingKind.Choice, "通常使用内置编码；已有 MLP 文件时可以选择导入。", ["surcode-batch", "external", "surcode"]),
        new("DVDA_MLP_SURCODE_SAMPLE_RATE", "采样率", "音频编码", SettingKind.Choice, "176.4 / 192 kHz 最多支持双声道，其余采样率最多六声道。", ["44100", "48000", "88200", "96000", "176400", "192000"]),
        new("DVDA_MLP_SURCODE_BITS", "音频位深", "音频编码", SettingKind.Choice, "通常选 24 位。降低位深可能丢失原音频精度。", ["16", "20", "24"]),
        new("DVDA_MLP_EAC3TO_EXE", "音源转换工具", "音频编码", SettingKind.File, "选择 eac3to.exe，用于读取音源和调整音频格式。"),
        new("DVDA_MLP_EXTERNAL_DIR", "MLP 文件夹", "音频编码", SettingKind.Folder, "内置编码可留空；导入已有 MLP 时请选择与音源结构对应的目录。"),
        new("DVDA_MLP_JOBS", "同时编码几首", "音频编码", SettingKind.Number, "默认 1。增加数量可同时处理更多音轨，也会占用更多内存。", Minimum: 1, Maximum: 16, Advanced: true),
        new("DVDA_MLP_METADATA_CONTEXT", "历史文件复现信息", "音频编码", SettingKind.File, "通常留空。仅在复现历史原版文件字节时选择对应的 stampctx 文件。", Advanced: true),
        new("DVDA_MLP_BATCH_TEMP_DIR", "音源转换临时位置", "音频编码", SettingKind.Folder, "留空使用工作文件夹。", Advanced: true),
        new("DVDA_MLP_BATCH_OUTPUT_DIR", "编码结果暂存位置", "音频编码", SettingKind.Folder, "留空使用工作文件夹。", Advanced: true),
        new("DVDA_MENU", "制作选曲菜单", "光盘菜单", SettingKind.Boolean, "在支持 DVD-Audio 的播放器中显示曲目菜单。"),
        new("DVDA_MENU_TRACKS_PER_PAGE", "每页显示几首", "光盘菜单", SettingKind.Number, "控制选曲菜单每页的曲目数量。", Minimum: 1, Maximum: 32),
        new("DVDA_MENU_STILLPICS", "显示专辑封面", "光盘菜单", SettingKind.Boolean, "将专辑图片作为静态封面加入光盘。"),
        new("DVDA_MENU_COVER_DIM", "封面暗化程度（%）", "光盘菜单", SettingKind.Number, "数值越大，封面越暗。", Maximum: 100),
        new("DVDA_MENU_INDEX_MIN_ALBUMS", "专辑目录启用门槛", "光盘菜单", SettingKind.Number, "达到此专辑数量时，添加专辑索引页。", Maximum: 1000, Advanced: true),
        new("DVDA_MENU_FONT", "中文菜单字体", "光盘菜单", SettingKind.File, "一般使用随程序附带的字体。", Advanced: true),
        new("DVDA_MENU_FONT_JP", "日文菜单字体", "光盘菜单", SettingKind.File, Advanced: true),
        new("DVDA_MENU_FONT_KR", "韩文菜单字体", "光盘菜单", SettingKind.File, Advanced: true),
        new("DVDA_AUTHOR", "光盘制作工具", "工具与高级", SettingKind.File, "dvda-author-dev.exe，通常由发布包自动配置。"),
        new("DVDA_MKISOFS", "镜像打包工具", "工具与高级", SettingKind.File, "mkisofs.exe，通常由发布包自动配置。"),
        new("DVDA_FFMPEG", "音源解码与校验", "工具与高级", SettingKind.File, "ffmpeg.exe，仅用于音源转换、解码与验证。"),
        new("DVDA_FFPROBE", "音频信息读取", "工具与高级", SettingKind.File, "ffprobe.exe。"),
        new("DVDA_METAFLAC", "FLAC 信息工具", "工具与高级", SettingKind.File, "metaflac.exe。"),
        new("DVDA_AUTHOR_SRC", "菜单素材文件夹", "工具与高级", SettingKind.Folder, "应包含 menu 子文件夹；发布包通常自动配置。"),
        new("DVDA_KEEP_TMP", "保留临时文件", "工具与高级", SettingKind.Boolean, "仅在排查问题时启用，会占用额外磁盘空间。", Advanced: true),
        new("DVDA_KEEP_INTERMEDIATE", "保留制作中间文件", "工具与高级", SettingKind.Boolean, "仅在排查问题时启用；启用后不复用之前的制作结果。", Advanced: true),
        new("DVDA_LOSS_WARN_S", "时长差异提醒（秒）", "工具与高级", SettingKind.Text, Advanced: true),
        new("DVDA_LOSS_ERROR_S", "时长差异上限（秒）", "工具与高级", SettingKind.Text, Advanced: true),
        new("DVDA_ALBUM_LIMIT", "仅处理前几个专辑", "工具与高级", SettingKind.Text, "留空处理全部专辑，仅用于诊断。", Advanced: true),
        new("DVDA_TITLE_MODE", "音频标题分组规则", "工具与高级", SettingKind.Text, "通常保留 album；诊断时可用 one 或正整数。", Advanced: true),
    ];
}
