using System.Diagnostics;
using DvdaMaker.Formats;
using DvdaMaker.Formats.Mlp;
using DvdaMaker.Formats.Mpeg;

internal static class NativeFormatSamples
{
    public static void Run(string root)
    {
        if (!Directory.Exists(root)) throw new DirectoryNotFoundException(root);
        var files = Directory.EnumerateFiles(root, "*.mlp", SearchOption.AllDirectories)
            .Order(StringComparer.OrdinalIgnoreCase).ToArray();
        if (files.Length == 0) throw new InvalidOperationException("No MLP samples were found.");

        var previous = Environment.GetEnvironmentVariable("DVDA_FORMATS_DISABLE_NATIVE");
        var watch = Stopwatch.StartNew();
        long bytes = 0;
        var checkedFiles = 0;
        try
        {
            foreach (var path in files)
            {
                var data = File.ReadAllBytes(path);
                bytes += data.Length;

                Environment.SetEnvironmentVariable("DVDA_FORMATS_DISABLE_NATIVE", "1");
                var managedInspection = MlpStreamAligner.Inspect(data);
                var managedAlignment = MlpStreamAligner.Align(data);

                Environment.SetEnvironmentVariable("DVDA_FORMATS_DISABLE_NATIVE", null);
                if (!NativeFormatsInterop.TryInspectFile(path, out var nativeInspection))
                    throw new InvalidOperationException($"Native formats DLL could not inspect {path}.");
                if (!NativeFormatsInterop.TryAlign(data, out var nativeAlignment))
                    throw new InvalidOperationException($"Native formats DLL could not align {path}.");

                Require(nativeInspection.IsValid == managedInspection.IsValid, path, "validity");
                Require(nativeInspection.Size == managedInspection.Size, path, "size");
                Require(nativeInspection.AccessUnitCount == managedInspection.AccessUnitCount, path, "AU count");
                Require(nativeInspection.MajorSyncCount == managedInspection.MajorSyncCount, path, "major sync count");
                Require(nativeInspection.HasEndOfStream == managedInspection.HasEndOfStream, path, "EOS");
                Require(nativeInspection.PeakBitrateRaw == managedInspection.PeakBitrateRaw, path, "peak");
                Require(nativeInspection.ExtendedSubstreamInfo == managedInspection.ExtendedSubstreamInfo, path, "extended info");
                Require(nativeInspection.SampleRate == managedInspection.SampleRate, path, "sample rate");
                Require(nativeAlignment.Data.AsSpan().SequenceEqual(managedAlignment.Data), path, "aligned bytes");
                Require(nativeAlignment.PeakChanges == managedAlignment.Changes.PeakBitrateChanges, path, "peak changes");
                Require(nativeAlignment.ExtendedChanges == managedAlignment.Changes.ExtendedSubstreamInfoChanges, path, "extended changes");
                Require(nativeAlignment.ChecksumChanges == managedAlignment.Changes.ChecksumChanges, path, "checksum changes");
                Require(nativeAlignment.InsertedEndOfStream == managedAlignment.Changes.AddedEndOfStream, path, "EOS changes");
                Require(nativeAlignment.OldHeader == managedAlignment.Changes.PreviousAccessUnitHeader, path, "old header");
                Require(nativeAlignment.NewHeader == managedAlignment.Changes.NewAccessUnitHeader, path, "new header");
                checkedFiles++;
            }

            Environment.SetEnvironmentVariable("DVDA_FORMATS_DISABLE_NATIVE", "1");
            var managedPts = new List<long>();
            foreach (var expected in new[] { 0L, 1L, 90_000L, 0x1_FFFF_FFFFL })
                managedPts.Add(PesTimestampParser.ParsePts(EncodePts(expected)));
            Environment.SetEnvironmentVariable("DVDA_FORMATS_DISABLE_NATIVE", null);
            foreach (var expected in managedPts)
            {
                var encoded = EncodePts(expected);
                if (!NativeFormatsInterop.TryParsePts(encoded, out var actual) || actual != expected)
                    throw new InvalidOperationException($"Native PTS mismatch: {expected} != {actual}.");
            }
        }
        finally
        {
            Environment.SetEnvironmentVariable("DVDA_FORMATS_DISABLE_NATIVE", previous);
        }

        Console.WriteLine($"native format samples: {checkedFiles}/{files.Length} MLP files, {bytes:N0} bytes, {watch.Elapsed}");
    }

    private static byte[] EncodePts(long value)
    {
        var encoded = new byte[5];
        encoded[0] = (byte)(0x21 | ((value >> 29) & 0x0E));
        encoded[1] = (byte)(value >> 22);
        encoded[2] = (byte)(((value >> 14) & 0xFE) | 1);
        encoded[3] = (byte)(value >> 7);
        encoded[4] = (byte)(((value << 1) & 0xFE) | 1);
        return encoded;
    }

    private static void Require(bool condition, string path, string field)
    {
        if (!condition) throw new InvalidOperationException($"Native sample mismatch ({field}): {path}");
    }
}
