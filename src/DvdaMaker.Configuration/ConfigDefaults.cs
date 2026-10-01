namespace DvdaMaker.Configuration;

public static class ConfigDefaults
{
    public const long Dvd5Bytes = 4_707_319_808;
    public const long Dvd9Bytes = 8_540_123_136;
    public const int GroupTrackHardLimit = 99;
    public const double AobOverhead = 1.025;
    public const long IsoSafety = 8L * 1024 * 1024;

    public static IReadOnlyDictionary<string, string> Values { get; } =
        new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["DVDA_SRC"] = "",
            ["DVDA_FINAL_DIR"] = "",
            ["DVDA_BUILD_DIR"] = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "DVD-Audio-Maker", "build"),
            ["DVDA_TITLE"] = "DVD-Audio",
            ["DVDA_ISO_PREFIX"] = "",
            ["DVDA_MAX_DISCS"] = "2",
            ["DVDA_GROUP_TRACK_LIMIT"] = "99",
            ["DVDA_DISC_BYTES"] = "",
            ["DVDA_MLP_SOURCE"] = "ffmpeg",
            ["DVDA_MLP_EXTERNAL_DIR"] = "",
            ["DVDA_MLP_BATCH_TEMP_DIR"] = "",
            ["DVDA_MLP_BATCH_OUTPUT_DIR"] = "",
            ["DVDA_MLP_SURCODE_EXE"] = "",
            ["DVDA_MLP_EAC3TO_EXE"] = "",
            ["DVDA_MLP_SURCODE_SAMPLE_RATE"] = "48000",
            ["DVDA_MLP_SURCODE_BITS"] = "24",
            ["DVDA_MLP_JOBS"] = "1",
            ["DVDA_MENU"] = "off",
            ["DVDA_MENU_TRACKS_PER_PAGE"] = "12",
            ["DVDA_MENU_INDEX_MIN_ALBUMS"] = "4",
            ["DVDA_MENU_STILLPICS"] = "on",
            ["DVDA_MENU_COVER_DIM"] = "35",
            ["DVDA_MENU_FONT"] = "Noto-Sans-CJK-SC",
            ["DVDA_MENU_FONT_JP"] = "",
            ["DVDA_MENU_FONT_KR"] = "",
            ["DVDA_AUTHOR"] = "dvda-author-dev.exe",
            ["DVDA_MKISOFS"] = "mkisofs.exe",
            ["DVDA_FFMPEG"] = "ffmpeg",
            ["DVDA_FFPROBE"] = "ffprobe",
            ["DVDA_METAFLAC"] = "metaflac",
            ["DVDA_AUTHOR_SRC"] = "",
            ["DVDA_PREPARE_CACHE"] = "on",
            ["DVDA_RESUME"] = "on",
            ["DVDA_LOSS_ERROR_S"] = "0.05",
            ["DVDA_LOSS_WARN_S"] = "0.005",
        };
}
