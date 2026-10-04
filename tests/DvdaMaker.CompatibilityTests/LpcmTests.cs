using DvdaMaker.Building;
using DvdaMaker.Configuration;
using DvdaMaker.SurcodeTool;

internal static class LpcmTests
{
    public static void FormatAndGrouping()
    {
        foreach (var mask in new uint[] { 4, 3, 0x103, 0x33, 0xb, 0x10b, 0x3b, 7, 0x107, 0x37, 0xf, 0x10f, 0x3f, 0, 0x1234, uint.MaxValue })
        {
            var supported = mask is not (0 or 0x1234 or uint.MaxValue);
            try { LpcmProvider.ValidateLayout(new SurcodePcmWav.WavLayout(24, 24, 2, 48000, 44, 6, 3, mask));
                if (!supported) throw new Exception("Invalid LPCM channel mask accepted"); }
            catch (InvalidDataException) { if (supported) throw; }
        }
        var source = Path.Combine(Path.GetTempPath(), "中文-日本語.flac");
        var hash = Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(
            System.Text.Encoding.UTF8.GetBytes(Path.GetFullPath(source).ToUpperInvariant())));
        if (LpcmProvider.CachePath("build", source) != Path.Combine("build", "lpcm", hash + ".wav"))
            throw new Exception("LPCM cache naming changed");
        foreach (var rate in new[] { 44100, 48000, 88200, 96000, 176400, 192000 })
            foreach (var bits in new[] { 16, 20, 24 })
                for (var channels = 1; channels <= 7; channels++)
                {
                    var valid = bits != 20 && channels <= 6 && (rate <= 96000 || channels <= 2) &&
                        (long)rate * bits * channels <= 9_600_000;
                    try { LpcmProvider.ValidateFormat(rate, bits, channels); if (!valid) throw new Exception("Invalid LPCM accepted"); }
                    catch (InvalidDataException) { if (valid) throw; }
                }
        var track = new BuildTrack { Date = "2026", Track = "1", Title = "First", Album = "Album",
            SampleRate = 48000, Bits = 24, SourcePath = "source.flac", ManifestName = "source", Duration = 1,
            SourceSize = 1, MlpPath = "one.wav", MlpSize = 100, MlpSource = "lpcm", Channels = 2 };
        var plan = new DiscPlanner().Plan([track, track with { Track = "2", Channels = 6, MlpPath = "two.wav" }],
            4_700_000_000, 0, 99);
        if (plan.HasErrors || plan.Discs.Single().Groups.Count != 2) throw new Exception("LPCM channels were mixed in a group");
        var mixed = new DiscPlanner().Plan([track with { Channels = 3, ChannelMask = 7 },
            track with { Track = "2", Channels = 3, ChannelMask = 0xb, MlpPath = "two.wav" }], 4_700_000_000, 0, 99);
        if (mixed.Discs.Single().Groups.Count != 2) throw new Exception("Different LPCM speaker layouts were mixed");
        var settings = ProjectSettings.Defaults(); settings.Values["DVDA_MLP_SOURCE"] = "lpcm";
        settings.Values["DVDA_MLP_SURCODE_BITS"] = "20";
        if (!settings.Validate(requireSource: false, requireEncoding: false).Any(e => e.Contains("LPCM")))
            throw new Exception("GUI accepted unsupported 20-bit LPCM");
    }
}
