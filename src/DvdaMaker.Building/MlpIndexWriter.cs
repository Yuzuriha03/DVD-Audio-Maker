using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;
using DvdaMaker.Configuration;

namespace DvdaMaker.Building;

public static class MlpIndexWriter
{
    public static void Write(
        string path,
        BuildPlan plan,
        DvdaOptions options,
        bool dryRun,
        DateTime? generated = null)
    {
        var duplicateMlpPaths = plan.Tracks
            .GroupBy(track => track.MlpPath, StringComparer.OrdinalIgnoreCase)
            .Where(group => group.Count() > 1)
            .Select(group => group.Key)
            .ToArray();
        if (duplicateMlpPaths.Length > 0)
        {
            throw new InvalidDataException(
                "多首曲目映射到同一个 MLP，索引会发生覆盖: " +
                string.Join(", ", duplicateMlpPaths));
        }

        var titleEnds = plan.Discs.SelectMany(disc => DvdaAuthorCommandBuilder.TitleEnds(disc, options.DiagnosticTitleMode))
            .ToDictionary(pair => pair.Key, pair => pair.Value, StringComparer.OrdinalIgnoreCase);
        var root = new JsonObject
        {
            ["__meta__"] = new JsonObject
            {
                ["generated"] = (generated ?? DateTime.Now).ToString("yyyy-MM-dd HH:mm:ss"),
                ["build_log"] = dryRun
                    ? Path.Combine(options.BuildDirectory, "build-dryrun.log")
                    : options.BuildLogPath,
                ["dry_run"] = dryRun,
                ["mlp_source"] = options.MlpSource,
                ["mlp_external_dir"] = string.IsNullOrEmpty(options.MlpExternalDirectory)
                    ? null
                    : options.MlpExternalDirectory,
                ["discs"] = plan.Discs.Count,
                ["tracks"] = plan.Tracks.Count,
                ["aob_layout"] = "第 N 组 -> AUDIO_TS/ATS_NN_1.AOB（超过 1 GiB 后续分段）",
            },
            ["__discs__"] = new JsonArray(plan.Discs.Select(disc =>
                (JsonNode)new JsonObject
                {
                    ["disc"] = disc.Number,
                    ["volid"] = options.VolumeId(disc.Number),
                    ["iso"] = options.IsoName(disc.Number),
                    ["groups"] = new JsonArray(disc.Groups.Select(group =>
                        (JsonNode)new JsonObject
                        {
                            ["group"] = group.Number,
                            ["sr"] = group.SampleRate,
                            ["bits"] = group.Bits,
                            ["aob"] = $"ATS_{group.Number:00}_1.AOB",
                            ["tracks"] = new JsonArray(group.Tracks.Select(TrackPlanNode).ToArray()),
                        }).ToArray()),
                }).ToArray()),
        };

        foreach (var track in plan.Tracks)
        {
            root[track.MlpPath] = new JsonObject
            {
                ["src"] = track.SourcePath,
                ["dur"] = track.Duration,
                ["sr"] = track.SampleRate,
                ["bits"] = track.Bits,
                ["ch"] = track.Channels,
                ["src_rate"] = track.SourceSampleRate,
                ["src_bits"] = track.SourceBits,
                ["resample_to"] = track.ResampleTo,
                ["mlp_source"] = track.MlpSource,
                ["ext_resampled"] = track.ExternalResampled,
                ["ext_rebitded"] = track.ExternalRebitded,
                ["param_changed"] = track.ParametersChanged,
                ["title"] = track.Title,
                ["lpcm_title_end"] = track.MlpSource == "lpcm" ? titleEnds[track.MlpPath] : null,
            };
        }

        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, root.ToJsonString(JsonOptions) + Environment.NewLine,
            new UTF8Encoding(false));
    }

    private static JsonNode TrackPlanNode(BuildTrack track) => new JsonObject
    {
        ["mlp"] = track.MlpPath,
        ["src"] = track.SourcePath,
        ["title"] = track.Title,
        ["resample_to"] = track.ResampleTo,
    };

    private static JsonSerializerOptions JsonOptions { get; } = new()
    {
        WriteIndented = true,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };
}
