using System.Buffers.Binary;
using System.Text;
using DvdaMaker.Toolchain;

internal static class NativeOptimizerTests
{
    public static void Run()
    {
        var root = Path.Combine(Path.GetTempPath(), "dvda-native-tests", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(root);
        try
        {
            var path = Path.Combine(root, "fixture.exe");
            File.WriteAllBytes(path, NativeFixture(delayed: false));
            Assert(NativeToolOptimizer.ReadImports(path).SetEquals(["kernel32.dll"]), "Normal import table");
            NativeToolOptimizer.ValidateShim(path);
            File.WriteAllBytes(path, NativeFixture(delayed: true));
            Assert(NativeToolOptimizer.ReadImports(path).SetEquals(["kernel32.dll", "late-library.dll"]), "Delay import table");
            Reject(() => NativeToolOptimizer.ValidateShim(path));
            var wrongArchitecture = NativeFixture(delayed: false);
            BinaryPrimitives.WriteUInt16LittleEndian(wrongArchitecture.AsSpan(0x84), 0x14c);
            File.WriteAllBytes(path, wrongArchitecture);
            Reject(() => NativeToolOptimizer.ValidateShim(path));
            Reject(() => NativeToolOptimizer.ValidateShim(typeof(NativeOptimizerTests).Assembly.Location));
            Reject(() => NativeToolOptimizer.ValidateMinimalFfmpeg(root));
            foreach (var name in new[] { "avcodec-63.dll", "avformat-63.dll", "avutil-61.dll" })
                File.WriteAllBytes(Path.Combine(root, name), NativeFixture(delayed: false));
            Reject(() => NativeToolOptimizer.ValidateMinimalFfmpeg(root));
            File.WriteAllBytes(Path.Combine(root, "magick.exe"), NativeFixture(delayed: false));
            File.WriteAllBytes(Path.Combine(root, "libraqm-0.dll"), [1, 2, 3]);
            Assert(NativeToolOptimizer.Optimize(root).Count == 0, "Unrecognized image tool is not pruned");
            Assert(File.ReadAllBytes(Path.Combine(root, "libraqm-0.dll")).SequenceEqual(new byte[] { 1, 2, 3 }),
                "Unknown library contents are retained");
        }
        finally { Directory.Delete(root, recursive: true); }
    }

    private static byte[] NativeFixture(bool delayed)
    {
        var bytes = new byte[1024];
        void U16(int offset, ushort value) => BinaryPrimitives.WriteUInt16LittleEndian(bytes.AsSpan(offset), value);
        void U32(int offset, uint value) => BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(offset), value);
        void Text(int offset, string value) => Encoding.ASCII.GetBytes(value).CopyTo(bytes, offset);
        Text(0, "MZ"); U32(0x3c, 0x80); Text(0x80, "PE\0\0");
        U16(0x84, 0x8664); U16(0x86, 1); U16(0x94, 240); U16(0x96, 0x22);
        const int optional = 0x98;
        U16(optional, 0x20b);
        BinaryPrimitives.WriteUInt64LittleEndian(bytes.AsSpan(optional + 24), 0x140000000);
        U32(optional + 32, 4096); U32(optional + 36, 512);
        U32(optional + 56, 8192); U32(optional + 60, 512); U32(optional + 108, 16);
        U32(optional + 112 + 8, 0x1000); U32(optional + 112 + 12, 40);
        Text(0x188, ".rdata"); U32(0x188 + 8, 512); U32(0x188 + 12, 0x1000);
        U32(0x188 + 16, 512); U32(0x188 + 20, 512); U32(0x188 + 36, 0x40000040);
        U32(512 + 12, 0x10a0); Text(512 + 0xa0, "KERNEL32.dll");
        if (delayed)
        {
            U32(optional + 112 + 13 * 8, 0x1040); U32(optional + 112 + 13 * 8 + 4, 64);
            U32(512 + 0x40, 1); U32(512 + 0x44, 0x10c0);
            Text(512 + 0xc0, "late-library.dll");
        }
        return bytes;
    }

    private static void Reject(Action action)
    {
        try { action(); }
        catch (InvalidDataException) { return; }
        throw new InvalidOperationException("Expected native executable validation to reject the fixture.");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
