namespace DvdaMaker.Building;

public static class DiscPublisher
{
    public static IReadOnlyList<string> PublishSet(
        IReadOnlyList<(string SourcePath, string FileName)> isoFiles,
        string finalDirectory,
        string pendingIndexPath,
        string formalIndexPath,
        bool moveStagedIsos = false)
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
        var finalRoot = Path.GetPathRoot(Path.GetFullPath(finalDirectory));
        var items = isoFiles.Select(item => new PublicationItem(
                item.SourcePath,
                Path.Combine(finalDirectory, item.FileName),
                Path.Combine(finalDirectory, item.FileName + $".publishing-{transaction}"),
                Path.Combine(finalDirectory, item.FileName + $".backup-{transaction}"),
                moveStagedIsos && string.Equals(
                    Path.GetPathRoot(Path.GetFullPath(item.SourcePath)), finalRoot,
                    StringComparison.OrdinalIgnoreCase)))
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
                if (item.MoveSource)
                {
                    // The unique transaction name must never replace an existing file.
                    File.Move(item.Source, item.Temporary);
                    item.SourceMoved = true;
                }
                else
                {
                    CopyToTemporaryVerified(item.Source, item.Temporary);
                }
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
        }
        catch
        {
            foreach (var item in items.Reverse())
            {
                try
                {
                    if (item.SourceMoved)
                    {
                        var current = item.Committed ? item.Destination : item.Temporary;
                        if (File.Exists(current)) File.Move(current, item.Source);
                    }
                    else if (item.Committed && File.Exists(item.Destination))
                    {
                        File.Delete(item.Destination);
                    }
                }
                catch (Exception rollbackException) when (
                    rollbackException is IOException or UnauthorizedAccessException)
                {
                }
                try
                {
                    if (item.BackupCreated && File.Exists(item.Backup) &&
                        !File.Exists(item.Destination))
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
                // A failed move rollback may leave the only new ISO at Temporary.
                if (!item.SourceMoved) TryDelete(item.Temporary);
            }
        }

        // The new set is committed. Cleanup failures must not trigger a rollback
        // after some old backups have already been removed.
        foreach (var item in items) TryDelete(item.Backup);
        TryDelete(pendingIndexPath);
        return isoFiles.Select(item => Path.Combine(finalDirectory, item.FileName)).ToArray();
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

    public static (string Path, BuildDiagnostic? Diagnostic) StageIso(
        string sourceIso,
        string stagingDirectory,
        string fileName)
    {
        var sourceRoot = Path.GetPathRoot(Path.GetFullPath(sourceIso));
        var stagingRoot = Path.GetPathRoot(Path.GetFullPath(stagingDirectory));
        if (!string.Equals(sourceRoot, stagingRoot, StringComparison.OrdinalIgnoreCase))
        {
            return Publish(sourceIso, stagingDirectory, fileName);
        }

        Directory.CreateDirectory(stagingDirectory);
        var staged = Path.Combine(stagingDirectory, fileName);
        // The transaction's staging directory is unique; never overwrite a staged ISO.
        File.Move(sourceIso, staged);
        return (staged, null);
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
        string backup,
        bool moveSource = false)
    {
        public string Source { get; } = source;
        public string Destination { get; } = destination;
        public string Temporary { get; } = temporary;
        public string Backup { get; } = backup;
        public bool MoveSource { get; } = moveSource;
        public bool SourceMoved { get; set; }
        public bool BackupCreated { get; set; }
        public bool Committed { get; set; }
    }
}
