using System.Security.Cryptography;
using DvdaMaker.Building;

internal static class PublicationMigrationTests
{
    public static void CompareTransactions()
    {
        var root = Path.Combine(Path.GetTempPath(), "dvda-rust-publication", Guid.NewGuid().ToString("N"));
        try
        {
            foreach (var move in new[] { false, true })
            foreach (var scenario in new[] { "success", "blocked-second", "blocked-index", "missing-source", "missing-index", "duplicate", "locked-second" })
            {
                var managed = Path.Combine(root, scenario + move, "managed");
                var rust = Path.Combine(root, scenario + move, "rust");
                Populate(managed, scenario); Populate(rust, scenario);
                var referenceResult = Run(managed, scenario, move, true);
                var rustResult = Run(rust, scenario, move, false);
                Equal(referenceResult, rustResult, scenario + " result");
                Equal(Snapshot(managed), Snapshot(rust), scenario + " filesystem");
            }
        }
        finally { if (Directory.Exists(root)) Directory.Delete(root, true); }
    }

    private static void Populate(string root, string scenario)
    {
        foreach (var dir in new[] { "stage", "final", "build" }) Directory.CreateDirectory(Path.Combine(root, dir));
        File.WriteAllText(Path.Combine(root, "stage", "一.iso"), "new-1");
        File.WriteAllText(Path.Combine(root, "stage", "二.iso"), "new-2");
        File.WriteAllText(Path.Combine(root, "final", "一.iso"), "old-1");
        File.WriteAllText(Path.Combine(root, "final", "二.iso"), "old-2");
        File.WriteAllText(Path.Combine(root, "build", "pending.json"), "new-index");
        File.WriteAllText(Path.Combine(root, "build", "formal.json"), "old-index");
        if (scenario == "blocked-second")
        { File.Delete(Path.Combine(root, "final", "二.iso")); Directory.CreateDirectory(Path.Combine(root, "final", "二.iso")); }
        if (scenario == "blocked-index")
        { File.Delete(Path.Combine(root, "build", "formal.json")); Directory.CreateDirectory(Path.Combine(root, "build", "formal.json")); }
        if (scenario == "missing-source") File.Delete(Path.Combine(root, "stage", "二.iso"));
        if (scenario == "missing-index") File.Delete(Path.Combine(root, "build", "pending.json"));
    }

    private static string Run(string root, string scenario, bool move, bool managed)
    {
        var inputs = new[] { (Path.Combine(root, "stage", "一.iso"), "一.iso"),
            (Path.Combine(root, "stage", "二.iso"), scenario == "duplicate" ? "一.ISO" : "二.iso") };
        using var locked = scenario == "locked-second"
            ? new FileStream(Path.Combine(root, "final", "二.iso"), FileMode.Open, FileAccess.Read, FileShare.None) : null;
        try
        {
            var result = managed
                ? DiscPublisher.PublishSetManaged(inputs, Path.Combine(root, "final"), Path.Combine(root, "build", "pending.json"), Path.Combine(root, "build", "formal.json"), move)
                : DiscPublisher.PublishSet(inputs, Path.Combine(root, "final"), Path.Combine(root, "build", "pending.json"), Path.Combine(root, "build", "formal.json"), move);
            return string.Join("|", result.Select(path => Path.GetRelativePath(root, path)));
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or InvalidDataException)
        { return error.GetType().Name; }
    }

    public static void CompareSingleFileOperations()
    {
        var root = Path.Combine(Path.GetTempPath(), "dvda-rust-publication-single", Guid.NewGuid().ToString("N"));
        try
        {
            foreach (var operation in new[] { "publish", "publish-fallback", "stage", "stage-existing", "copy", "copy-existing" })
            {
                var managed = Path.Combine(root, operation, "managed");
                var rust = Path.Combine(root, operation, "rust");
                string RunSingle(string directory, bool reference)
                {
                    Directory.CreateDirectory(Path.Combine(directory, "final"));
                    var source = Path.Combine(directory, "source.iso");
                    var target = Path.Combine(directory, "final", "disc.iso");
                    File.WriteAllBytes(source, [0, 1, 2, 255]);
                    if (operation.EndsWith("existing")) File.WriteAllBytes(target, [99, 88]);
                    if (operation == "publish-fallback") Directory.CreateDirectory(target);
                    try
                    {
                        if (operation.StartsWith("publish"))
                        {
                            var value = reference ? DiscPublisher.PublishManaged(source, Path.GetDirectoryName(target)!, "disc.iso")
                                : DiscPublisher.Publish(source, Path.GetDirectoryName(target)!, "disc.iso");
                            return Path.GetFileName(value.Path) + "|" + value.Diagnostic?.Code;
                        }
                        if (operation.StartsWith("stage"))
                        {
                            var value = reference ? DiscPublisher.StageIsoManaged(source, Path.GetDirectoryName(target)!, "disc.iso")
                                : DiscPublisher.StageIso(source, Path.GetDirectoryName(target)!, "disc.iso");
                            return Path.GetFileName(value.Path) + "|" + value.Diagnostic?.Code;
                        }
                        if (reference) DiscPublisher.CopyVerifiedManaged(source, target); else DiscPublisher.CopyVerified(source, target);
                        return "copied";
                    }
                    catch (Exception error) when (error is IOException or UnauthorizedAccessException) { return error.GetType().Name; }
                }
                Equal(RunSingle(managed, true), RunSingle(rust, false), operation + " result");
                Equal(Snapshot(managed), Snapshot(rust), operation + " filesystem");
            }
        }
        finally { if (Directory.Exists(root)) Directory.Delete(root, true); }
    }

    private static string Snapshot(string root) => string.Join("\n",
        Directory.EnumerateFileSystemEntries(root, "*", SearchOption.AllDirectories).Order(StringComparer.Ordinal)
            .Select(path => Path.GetRelativePath(root, path) + (Directory.Exists(path) ? "|directory"
                : "|" + Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path))))));

    private static void Equal(string expected, string actual, string context)
    { if (expected != actual) throw new InvalidOperationException($"{context}: expected {expected}, actual {actual}"); }
}
