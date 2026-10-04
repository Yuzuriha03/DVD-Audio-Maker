using System.Buffers.Binary;

namespace DvdaMaker.Formats.Mlp;

public static class MlpStreamAligner
{
    private static ReadOnlySpan<byte> MajorSyncSignature => [0xF8, 0x72, 0x6F];
    private static ReadOnlySpan<byte> EndOfStreamBytes => [0xD2, 0x34, 0xD2, 0x34];

    public const int MajorSyncSize = 28;
    public const int BasePeakBitrate = 9_600_000;

    private static readonly uint[] Crc2D = CreateCrcTable(0x002D, 16);
    private static readonly uint[] Crc63 = CreateCrcTable(0x0063, 8);

    public static IReadOnlyList<MlpAccessUnit> Walk(ReadOnlySpan<byte> data)
    {
        var position = 0;
        var output = new List<MlpAccessUnit>();
        while (position + 4 <= data.Length)
        {
            var header = BinaryPrimitives.ReadUInt16BigEndian(data.Slice(position, 2));
            var length = (header & 0x0FFF) * 2;
            if (length < 4 || position + length > data.Length)
            {
                throw new InvalidDataException(
                    $"access unit 在 offset {position} 处长度非法 ({length})");
            }

            var hasMajorSync = position + 7 <= data.Length &&
                               data.Slice(position + 4, 3).SequenceEqual(MajorSyncSignature);
            output.Add(new MlpAccessUnit(position, length, hasMajorSync));
            position += length;
        }

        if (position != data.Length)
        {
            throw new InvalidDataException(
                $"access unit 未覆盖整个文件（余 {data.Length - position} 字节）");
        }
        return output;
    }

    public static int? SampleRateOf(ReadOnlySpan<byte> majorSync)
    {
        if (NativeFormatsInterop.TrySampleRate(majorSync, out var nativeRate))
        {
            return nativeRate;
        }
        if (majorSync.Length <= 5)
        {
            return null;
        }

        var rateBits = (majorSync[5] >> 4) & 0x0F;
        if (rateBits == 0x0F)
        {
            return null;
        }
        return ((rateBits & 8) != 0 ? 44_100 : 48_000) << (rateBits & 7);
    }

    public static int PeakBitrateRaw(int sampleRate)
    {
        if (sampleRate <= 0)
        {
            throw new ArgumentOutOfRangeException(nameof(sampleRate));
        }
        if (NativeFormatsInterop.TryPeakBitrate(sampleRate, out var nativePeak))
        {
            return nativePeak;
        }
        var numerator = (long)BasePeakBitrate * 16 - 8;
        return checked((int)((numerator + sampleRate - 1) / sampleRate));
    }

    public static ushort Checksum16(ReadOnlySpan<byte> buffer, int size)
    {
        if (size < 2 || size > buffer.Length)
        {
            throw new ArgumentOutOfRangeException(nameof(size));
        }
        if (NativeFormatsInterop.TryChecksum16(buffer[..size], out var nativeChecksum))
        {
            return nativeChecksum;
        }
        var crc = AvCrc(Crc2D, 0, buffer, size - 2);
        crc ^= BinaryPrimitives.ReadUInt16LittleEndian(buffer.Slice(size - 2, 2));
        return (ushort)crc;
    }

    public static byte Checksum8(ReadOnlySpan<byte> buffer, int size)
    {
        if (size < 1 || size > buffer.Length)
        {
            throw new ArgumentOutOfRangeException(nameof(size));
        }
        if (NativeFormatsInterop.TryChecksum8(buffer[..size], out var nativeChecksum))
        {
            return nativeChecksum;
        }
        var crc = AvCrc(Crc63, 0x3C, buffer, size - 1);
        crc ^= buffer[size - 1];
        return (byte)crc;
    }

    public static byte CalculateParity(ReadOnlySpan<byte> buffer)
    {
        if (NativeFormatsInterop.TryCalculateParity(buffer, out var nativeParity))
        {
            return nativeParity;
        }
        var parity = 0;
        foreach (var value in buffer)
        {
            parity ^= value;
        }
        parity ^= parity >> 16;
        parity ^= parity >> 8;
        return (byte)parity;
    }

    public static MlpInspection Inspect(ReadOnlySpan<byte> data)
    {
        if (NativeFormatsInterop.TryInspect(data, out var native) && native.IsValid)
        {
            return FromNative(native);
        }
        var units = Walk(data);
        var state = new InspectionAccumulator();
        foreach (var unit in units)
        {
            state.AddUnit(
                data.Slice(unit.Offset, unit.Length),
                unit.Offset,
                unit.HasMajorSync);
        }
        return state.Complete(data.Length, units.Count);
    }

    /// <summary>
    /// Inspects an MLP stream without loading the complete file into memory. Each
    /// access unit is bounded by the 12-bit length field, so validation remains
    /// bounded even for very large imported MLP files.
    /// </summary>
    public static MlpInspection Inspect(Stream stream)
    {
        ArgumentNullException.ThrowIfNull(stream);
        var state = new InspectionAccumulator();
        var header = new byte[4];
        long size = 0;
        var units = 0;
        while (true)
        {
            var headerBytes = ReadAtMost(stream, header);
            if (headerBytes == 0)
            {
                break;
            }
            if (headerBytes != header.Length)
            {
                throw new InvalidDataException("MLP access unit header is truncated.");
            }
            var length = (BinaryPrimitives.ReadUInt16BigEndian(header.AsSpan(0, 2)) & 0x0FFF) * 2;
            if (length < 4)
            {
                throw new InvalidDataException($"MLP access unit at offset {size} has invalid length {length}.");
            }
            if (size > int.MaxValue - length)
            {
                throw new InvalidDataException("MLP stream is too large for the inspection model.");
            }
            var unit = new byte[length];
            header.CopyTo(unit, 0);
            ReadExactly(stream, unit.AsSpan(4));
            var hasMajorSync = length >= 7 && unit.AsSpan(4, 3).SequenceEqual(MajorSyncSignature);
            state.AddUnit(unit, (int)size, hasMajorSync);
            size += length;
            units++;
        }
        return state.Complete((int)size, units);
    }

    public static async Task<MlpInspection> InspectAsync(
        Stream stream,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(stream);
        var state = new InspectionAccumulator();
        var header = new byte[4];
        long size = 0;
        var units = 0;
        while (true)
        {
            var headerBytes = await ReadAtMostAsync(stream, header, cancellationToken).ConfigureAwait(false);
            if (headerBytes == 0)
            {
                break;
            }
            if (headerBytes != header.Length)
            {
                throw new InvalidDataException("MLP access unit header is truncated.");
            }
            var length = (BinaryPrimitives.ReadUInt16BigEndian(header.AsSpan(0, 2)) & 0x0FFF) * 2;
            if (length < 4)
            {
                throw new InvalidDataException($"MLP access unit at offset {size} has invalid length {length}.");
            }
            if (size > int.MaxValue - length)
            {
                throw new InvalidDataException("MLP stream is too large for the inspection model.");
            }
            var unit = new byte[length];
            header.CopyTo(unit, 0);
            await ReadExactlyAsync(stream, unit.AsMemory(4), cancellationToken).ConfigureAwait(false);
            var hasMajorSync = length >= 7 && unit.AsSpan(4, 3).SequenceEqual(MajorSyncSignature);
            state.AddUnit(unit, (int)size, hasMajorSync);
            size += length;
            units++;
        }
        return state.Complete((int)size, units);
    }

    public static MlpInspection InspectFile(string path)
    {
        if (NativeFormatsInterop.TryInspectFile(path, out var native) && native.IsValid)
        {
            return FromNative(native);
        }
        using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read,
            bufferSize: 128 * 1024, options: FileOptions.SequentialScan);
        return Inspect(stream);
    }

    public static async Task<MlpInspection> InspectFileAsync(
        string path,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (NativeFormatsInterop.TryInspectFile(path, out var native) && native.IsValid)
        {
            return FromNative(native);
        }
        await using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read,
            bufferSize: 128 * 1024, options: FileOptions.Asynchronous | FileOptions.SequentialScan);
        return await InspectAsync(stream, cancellationToken).ConfigureAwait(false);
    }
    public static MlpAlignmentResult Align(ReadOnlySpan<byte> data)
    {
        if (NativeFormatsInterop.TryAlign(data, out var native))
        {
            return new MlpAlignmentResult(
                native.Data,
                new MlpAlignmentChanges(
                    native.PeakChanges,
                    native.ExtendedChanges,
                    native.ChecksumChanges,
                    native.InsertedEndOfStream,
                    native.OldHeader,
                    native.NewHeader));
        }
        var buffer = data.ToArray();
        var units = Walk(data);
        if (units.Count == 0)
        {
            throw new InvalidDataException("MLP 文件不包含 access unit。");
        }

        var peakChanges = 0;
        var extendedChanges = 0;
        var checksumChanges = 0;

        foreach (var unit in units.Where(unit => unit.HasMajorSync))
        {
            var offset = unit.Offset + 4;
            if (offset + MajorSyncSize > buffer.Length)
            {
                throw new InvalidDataException($"offset {offset} 的 major sync 被截断");
            }
            var majorSync = buffer.AsSpan(offset, MajorSyncSize);
            var sampleRate = SampleRateOf(majorSync) ??
                throw new InvalidDataException($"offset {offset} 的 major sync 采样率未知");
            var wantedPeak = PeakBitrateRaw(sampleRate);
            var value = BinaryPrimitives.ReadUInt16BigEndian(majorSync[14..16]);
            if ((value & 0x7FFF) != wantedPeak)
            {
                value = (ushort)((value & 0x8000) | (wantedPeak & 0x7FFF));
                BinaryPrimitives.WriteUInt16BigEndian(majorSync[14..16], value);
                peakChanges++;
            }
            if ((majorSync[16] & 3) != 1)
            {
                majorSync[16] = (byte)((majorSync[16] & 0xFC) | 1);
                extendedChanges++;
            }

            var checksum = Checksum16(majorSync, MajorSyncSize - 2);
            if (BinaryPrimitives.ReadUInt16LittleEndian(majorSync[26..28]) != checksum)
            {
                BinaryPrimitives.WriteUInt16LittleEndian(majorSync[26..28], checksum);
                checksumChanges++;
            }
        }

        var last = units[^1];
        var substreamHeaderOffset = last.Offset + 4 + (last.HasMajorSync ? MajorSyncSize : 0);
        if (substreamHeaderOffset + 2 > buffer.Length)
        {
            throw new InvalidDataException("最后一个 access unit 缺少子流头。");
        }
        var substreamHeader = BinaryPrimitives.ReadUInt16BigEndian(
            buffer.AsSpan(substreamHeaderOffset, 2));
        var end = (substreamHeader & 0x0FFF) * 2;
        var substreamDataOffset = substreamHeaderOffset + 2;
        if (end < 2 || substreamDataOffset + end > buffer.Length)
        {
            throw new InvalidDataException("最后一个 access unit 的子流长度非法。");
        }

        var bodyLength = end - 2;
        var body = buffer.AsSpan(substreamDataOffset, bodyLength);
        if (body.EndsWith(EndOfStreamBytes))
        {
            return new MlpAlignmentResult(
                buffer,
                new MlpAlignmentChanges(
                    peakChanges, extendedChanges, checksumChanges, false, null, null));
        }

        var newBuffer = new byte[buffer.Length + 4];
        var insertionOffset = substreamDataOffset + bodyLength;
        buffer.AsSpan(0, insertionOffset).CopyTo(newBuffer);
        EndOfStreamBytes.CopyTo(newBuffer.AsSpan(insertionOffset, 4));
        buffer.AsSpan(insertionOffset + 2).CopyTo(newBuffer.AsSpan(insertionOffset + 6));

        var newBodyLength = bodyLength + 4;
        var newBody = newBuffer.AsSpan(substreamDataOffset, newBodyLength);
        newBuffer[substreamDataOffset + newBodyLength] = (byte)(CalculateParity(newBody) ^ 0xA9);
        newBuffer[substreamDataOffset + newBodyLength + 1] = Checksum8(newBody, newBody.Length);

        substreamHeader = (ushort)((substreamHeader & 0xF000) |
                                   (((substreamHeader & 0x0FFF) + 2) & 0x0FFF));
        BinaryPrimitives.WriteUInt16BigEndian(
            newBuffer.AsSpan(substreamHeaderOffset, 2), substreamHeader);

        var timing = BinaryPrimitives.ReadUInt16BigEndian(newBuffer.AsSpan(last.Offset + 2, 2));
        var newLengthWords = (last.Length / 2) + 2;
        var parityNibble = timing ^ newLengthWords;
        parityNibble ^= (substreamHeader >> 8) & 0xFF;
        parityNibble ^= substreamHeader & 0xFF;
        parityNibble ^= parityNibble >> 8;
        parityNibble ^= parityNibble >> 4;
        parityNibble &= 0xF;
        var oldHeader = BinaryPrimitives.ReadUInt16BigEndian(newBuffer.AsSpan(last.Offset, 2));
        var newHeader = (ushort)(((parityNibble ^ 0xF) << 12) | (newLengthWords & 0x0FFF));
        BinaryPrimitives.WriteUInt16BigEndian(newBuffer.AsSpan(last.Offset, 2), newHeader);

        return new MlpAlignmentResult(
            newBuffer,
            new MlpAlignmentChanges(
                peakChanges,
                extendedChanges,
                checksumChanges,
                true,
                oldHeader,
                newHeader));
    }

    private static MlpInspection FromNative(NativeFormatsInterop.NativeMlpInspection native) => new(
        native.Size,
        native.AccessUnitCount,
        native.MajorSyncCount,
        native.MajorSyncInterval,
        [],
        [],
        [],
        native.HasEndOfStream,
        native.PeakBitrateRaw,
        native.ExtendedSubstreamInfo,
        native.SampleRate);

    private sealed class InspectionAccumulator
    {
        private readonly List<MlpMajorSyncError> _majorSyncErrors = [];
        private readonly List<MlpAuParityError> _parityErrors = [];
        private readonly List<int> _substreamErrors = [];
        private int _majorSyncCount;
        private int? _peakRaw;
        private int? _extendedInfo;
        private int? _sampleRate;
        private bool _lastHasEndOfStream;

        public void AddUnit(ReadOnlySpan<byte> unit, int offset, bool hasMajorSync)
        {
            _lastHasEndOfStream = false;
            if (hasMajorSync)
            {
                _majorSyncCount++;
                var majorOffset = offset + 4;
                if (4 + MajorSyncSize > unit.Length)
                {
                    _majorSyncErrors.Add(new MlpMajorSyncError(majorOffset, "截断"));
                }
                else
                {
                    var majorSync = unit.Slice(4, MajorSyncSize);
                    if (BinaryPrimitives.ReadUInt16BigEndian(majorSync[8..10]) != 0xB752)
                    {
                        _majorSyncErrors.Add(new MlpMajorSyncError(
                            majorOffset,
                            $"INFO_SIG=0x{BinaryPrimitives.ReadUInt16BigEndian(majorSync[8..10]):x4}"));
                    }
                    var storedChecksum = BinaryPrimitives.ReadUInt16LittleEndian(majorSync[26..28]);
                    if (Checksum16(majorSync, MajorSyncSize - 2) != storedChecksum)
                    {
                        _majorSyncErrors.Add(new MlpMajorSyncError(majorOffset, "checksum16 不符"));
                    }
                    var currentPeak = BinaryPrimitives.ReadUInt16BigEndian(majorSync[14..16]) & 0x7FFF;
                    var currentExtendedInfo = majorSync[16] & 3;
                    var currentSampleRate = SampleRateOf(majorSync);
                    if (currentSampleRate is null)
                    {
                        _majorSyncErrors.Add(new MlpMajorSyncError(majorOffset, "采样率未知"));
                    }
                    else if (currentPeak < (long)BasePeakBitrate * 16 / currentSampleRate.Value ||
                             currentPeak > PeakBitrateRaw(currentSampleRate.Value))
                    {
                        _majorSyncErrors.Add(new MlpMajorSyncError(
                            majorOffset,
                            $"peak={currentPeak}，期望 {PeakBitrateRaw(currentSampleRate.Value)}"));
                    }
                    if (currentExtendedInfo != 1)
                    {
                        _majorSyncErrors.Add(new MlpMajorSyncError(
                            majorOffset,
                            $"extended_substream_info={currentExtendedInfo}，期望 1"));
                    }
                    if (_sampleRate is not null && currentSampleRate != _sampleRate)
                    {
                        _majorSyncErrors.Add(new MlpMajorSyncError(majorOffset, "major sync 采样率不一致"));
                    }
                    if (_peakRaw is not null && currentPeak != _peakRaw)
                    {
                        _majorSyncErrors.Add(new MlpMajorSyncError(majorOffset, "major sync peak 字段不一致"));
                    }
                    if (_extendedInfo is not null && currentExtendedInfo != _extendedInfo)
                    {
                        _majorSyncErrors.Add(new MlpMajorSyncError(majorOffset, "major sync ext 字段不一致"));
                    }
                    _peakRaw ??= currentPeak;
                    _extendedInfo ??= currentExtendedInfo;
                    _sampleRate ??= currentSampleRate;
                }
            }

            var headerOffset = 4 + (hasMajorSync ? MajorSyncSize : 0);
            if (headerOffset + 2 > unit.Length)
            {
                _substreamErrors.Add(offset);
                return;
            }

            var substreamHeader = BinaryPrimitives.ReadUInt16BigEndian(unit.Slice(headerOffset, 2));
            var end = (substreamHeader & 0x0FFF) * 2;
            var dataOffset = headerOffset + 2;
            if (end < 2 || dataOffset + end > unit.Length)
            {
                _substreamErrors.Add(offset);
                return;
            }
            var substreamData = unit.Slice(dataOffset, end);

            var timing = BinaryPrimitives.ReadUInt16BigEndian(unit.Slice(2, 2));
            var parity = timing ^ (unit.Length / 2);
            parity ^= (substreamHeader >> 8) & 0xFF;
            parity ^= substreamHeader & 0xFF;
            parity ^= parity >> 8;
            parity ^= parity >> 4;
            parity &= 0xF;
            var expected = (unit[0] >> 4) & 0xF;
            var actual = parity ^ 0xF;
            if (actual != expected)
            {
                _parityErrors.Add(new MlpAuParityError(offset, expected, actual));
            }

            if (((substreamHeader >> 13) & 1) != 0)
            {
                if (substreamData.Length < 2)
                {
                    _substreamErrors.Add(offset);
                    return;
                }
                var body = substreamData[..^2];
                if (substreamData[^2] != (CalculateParity(body) ^ 0xA9) ||
                    substreamData[^1] != Checksum8(body, body.Length))
                {
                    _substreamErrors.Add(offset);
                }
            }

            _lastHasEndOfStream = substreamData.Length >= 6 &&
                substreamData[..^2].EndsWith(EndOfStreamBytes);
        }

        public MlpInspection Complete(int size, int unitCount) => new(
            size,
            unitCount,
            _majorSyncCount,
            _majorSyncCount == 0 ? 0 : (double)unitCount / _majorSyncCount,
            _majorSyncErrors,
            _parityErrors,
            _substreamErrors,
            _lastHasEndOfStream,
            _peakRaw,
            _extendedInfo,
            _sampleRate);
    }

    private static int ReadAtMost(Stream stream, Span<byte> buffer)
    {
        var total = 0;
        while (total < buffer.Length)
        {
            var read = stream.Read(buffer[total..]);
            if (read == 0)
            {
                break;
            }
            total += read;
        }
        return total;
    }

    private static async Task<int> ReadAtMostAsync(
        Stream stream,
        Memory<byte> buffer,
        CancellationToken cancellationToken)
    {
        var total = 0;
        while (total < buffer.Length)
        {
            var read = await stream.ReadAsync(buffer[total..], cancellationToken).ConfigureAwait(false);
            if (read == 0)
            {
                break;
            }
            total += read;
        }
        return total;
    }

    private static void ReadExactly(Stream stream, Span<byte> buffer)
    {
        var total = 0;
        while (total < buffer.Length)
        {
            var read = stream.Read(buffer[total..]);
            if (read == 0)
            {
                throw new InvalidDataException("MLP access unit is truncated.");
            }
            total += read;
        }
    }

    private static async Task ReadExactlyAsync(
        Stream stream,
        Memory<byte> buffer,
        CancellationToken cancellationToken)
    {
        var total = 0;
        while (total < buffer.Length)
        {
            var read = await stream.ReadAsync(buffer[total..], cancellationToken).ConfigureAwait(false);
            if (read == 0)
            {
                throw new InvalidDataException("MLP access unit is truncated.");
            }
            total += read;
        }
    }
    private static uint[] CreateCrcTable(uint polynomial, int bits)
    {
        var table = new uint[256];
        for (var index = 0; index < table.Length; index++)
        {
            var value = (uint)index << 24;
            for (var bit = 0; bit < 8; bit++)
            {
                value = (value << 1) ^
                        ((value >> 31) != 0 ? polynomial << (32 - bits) : 0);
            }
            table[index] = BinaryPrimitives.ReverseEndianness(value);
        }
        return table;
    }

    private static uint AvCrc(
        IReadOnlyList<uint> table,
        uint crc,
        ReadOnlySpan<byte> buffer,
        int count)
    {
        for (var index = 0; index < count; index++)
        {
            crc = table[(int)((crc & 0xFF) ^ buffer[index])] ^ (crc >> 8);
        }
        return crc;
    }
}
