using DvdaMaker.Building;
using DvdaMaker.Configuration;

internal static class LpcmTests
{
    public static void FormatAndGrouping()
    {
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
        var settings = ProjectSettings.Defaults(); settings.Values["DVDA_MLP_SOURCE"] = "lpcm";
        settings.Values["DVDA_MLP_SURCODE_BITS"] = "20";
        if (!settings.Validate(requireSource: false, requireEncoding: false).Any(e => e.Contains("LPCM")))
            throw new Exception("GUI accepted unsupported 20-bit LPCM");
    }
}
