namespace DvdaMaker.Formats.Iso9660;

public sealed record IsoDirectoryEntry(
    string Name,
    bool IsDirectory,
    uint LogicalBlockAddress,
    uint Size,
    byte ExtendedAttributeBlocks,
    bool IsMultiExtent);
