namespace DvdaMaker.Formats.Iso9660;

public sealed class Iso9660Exception : Exception
{
    public Iso9660Exception(string message) : base(message)
    {
    }

    public Iso9660Exception(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}
