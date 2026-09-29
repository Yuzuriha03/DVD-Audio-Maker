namespace DvdaMaker.Building;

public static class DiscPublisher
{
    public static IReadOnlyList<string> PublishSet(
        IReadOnlyList<(string SourcePath, string FileName)> isoFiles,
        string finalDirectory,
        string pendingIndexPath,
        string formalIndexPath)
    {
        var duplicateNames = isoFiles
            .GroupBy(item => item.FileName, StringComparer.OrdinalIgnoreCase)
            .Where(group => group.Count() > 1)
            .Select(group => group.Key)
            .ToArray();
        if (duplicateNames.Length > 0)
        {
            throw new InvalidDataException(
                "发布集合包含重复 ISO 文件名: " + string.Join(", ", duplicateNames));
        }

        Directory.CreateDirectory(finalDirectory);
        Directory.CreateDirectory(Path.GetDirectoryName(formalIndexPath)!);
        var transaction = Guid.NewGuid().ToString("N");
        var items = isoFiles.Select(item => new PublicationItem(
                item.SourcePath,
                Path.Combine(finalDirectory, item.FileName),
                Path.Combine(finalDirectory, item.FileName + $".publishing-{transaction}"),
                Path.Combine(finalDirectory, item.FileName + $".backup-{transaction}")))
            .Append(new PublicationItem(
                pendingIndexPath,
                formalIndexPath,
                formalIndexPath + $".publishing-{transaction}",
                formalIndexPath + $".backup-{transaction}"))
            .ToArray();

        try
        {
            foreach (var item in items)
            {
                CopyToTemporaryVerified(item.Source, item.Temporary);
            }

            foreach (var item in items)
            {
                if (File.Exists(item.Destination))
                {
                    File.Move(item.Destination, item.Backup);
                    item.BackupCreated = true;
                }
                File.Move(item.Temporary, item.Destination);
                item.Committed = true;
            }

            foreach (var item in items)
            {
                if (item.BackupCreated && File.Exists(item.Backup)) File.Delete(item.Backup);
            }
            if (File.Exists(pendingIndexPath)) File.Delete(pendingIndexPath);
            return isoFiles.Select(item => Path.Combine(finalDirectory, item.FileName)).ToArray();
        }
        catch
        {
            foreach (var item in items.Reverse())
            {
                try
                {
                    if (item.Committed && File.Exists(item.Destination))
                    {
                        File.Delete(item.Destination);
                    }
                    if (item.BackupCreated && File.Exists(item.Backup))
                    {
                        File.Move(item.Backup, item.Destination);
                    }
                }
                catch (Exception rollbackException) when (
                    rollbackException is IOException or UnauthorizedAccessException)
                {
                }
            }
            throw;
        }
        finally
        {
            foreach (var item in items)
            {
                TryDelete(item.Temporary);
                TryDelete(item.Backup);
            }
        }
    }

    public static (string Path, BuildDiagnostic? Diagnostic) Publish(
        string sourceIso,
        string finalDirectory,
        string fileName)
    {
        Directory.CreateDirectory(finalDirectory);
        var destination = Path.Combine(finalDirectory, fileName);
        try
        {
            CopyVerified(sourceIso, destination);
            return (destination, null);
        }
        catch (IOException exception)
        {
            var alternative = Path.Combine(
                finalDirectory,
                Path.GetFileNameWithoutExtension(fileName) + "_new.iso");
            CopyVerified(sourceIso, alternative);
            return (alternative, new BuildDiagnostic(
                BuildDiagnosticSeverity.Warning,
                "FINAL_ISO_IN_USE",
                $"无法覆盖 {fileName}（{exception.Message}），已改写为 {Path.GetFileName(alternative)}。"));
        }
        catch (UnauthorizedAccessException exception)
        {
            var alternative = Path.Combine(
                finalDirectory,
                Path.GetFileNameWithoutExtension(fileName) + "_new.iso");
            CopyVerified(sourceIso, alternative);
            return (alternative, new BuildDiagnostic(
                BuildDiagnosticSeverity.Warning,
                "FINAL_ISO_IN_USE",
                $"无法覆盖 {fileName}（{exception.Message}），已改写为 {Path.GetFileName(alternative)}。"));
        }
    }

    public static void CopyVerified(string source, string destination)
    {
        var temporary = destination + ".copying";
        CopyToTemporaryVerified(source, temporary);
        File.Move(temporary, destination, overwrite: true);
    }

    private static void CopyToTemporaryVerified(string source, string temporary)
    {
        TryDelete(temporary);
        File.Copy(source, temporary, overwrite: true);
        var sourceLength = new FileInfo(source).Length;
        var copiedLength = new FileInfo(temporary).Length;
        if (sourceLength != copiedLength)
        {
            TryDelete(temporary);
            throw new IOException($"文件复制长度不一致: {sourceLength} != {copiedLength}");
        }
    }

    private static void TryDelete(string path)
    {
        try
        {
            if (File.Exists(path)) File.Delete(path);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
        }
    }

    private sealed class PublicationItem(
        string source,
        string destination,
        string temporary,
        string backup)
    {
        public string Source { get; } = source;
        public string Destination { get; } = destination;
        public string Temporary { get; } = temporary;
        public string Backup { get; } = backup;
        public bool BackupCreated { get; set; }
        public bool Committed { get; set; }
    }
}
