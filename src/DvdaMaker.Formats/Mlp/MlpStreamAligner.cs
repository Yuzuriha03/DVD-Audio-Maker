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
        var numerator = (long)BasePeakBitrate * 16 - 8;
        return checked((int)((numerator + sampleRate - 1) / sampleRate));
    }

    public static ushort Checksum16(ReadOnlySpan<byte> buffer, int size)
    {
        if (size < 2 || size > buffer.Length)
        {
            throw new ArgumentOutOfRangeException(nameof(size));
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
        var crc = AvCrc(Crc63, 0x3C, buffer, size - 1);
        crc ^= buffer[size - 1];
        return (byte)crc;
    }

    public static byte CalculateParity(ReadOnlySpan<byte> buffer)
    {
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
        var units = Walk(data);
        var majorSyncOffsets = units.Where(unit => unit.HasMajorSync)
            .Select(unit => unit.Offset + 4)
            .ToArray();
        var majorSyncErrors = new List<MlpMajorSyncError>();
        var parityErrors = new List<MlpAuParityError>();
        var substreamErrors = new List<int>();
        int? peakRaw = null;
        int? extendedInfo = null;
        int? sampleRate = null;
        var hasEndOfStream = false;

        foreach (var unit in units.Where(unit => unit.HasMajorSync))
        {
            var offset = unit.Offset + 4;
            var unitEnd = unit.Offset + unit.Length;
            if (offset + MajorSyncSize > unitEnd)
            {
                majorSyncErrors.Add(new MlpMajorSyncError(offset, "截断"));
                continue;
            }
            var majorSync = data.Slice(offset, MajorSyncSize);
            if (BinaryPrimitives.ReadUInt16BigEndian(majorSync[8..10]) != 0xB752)
            {
                majorSyncErrors.Add(new MlpMajorSyncError(
                    offset,
                    $"INFO_SIG=0x{BinaryPrimitives.ReadUInt16BigEndian(majorSync[8..10]):x4}"));
            }
            var storedChecksum = BinaryPrimitives.ReadUInt16LittleEndian(majorSync[26..28]);
            if (Checksum16(majorSync, MajorSyncSize - 2) != storedChecksum)
            {
                majorSyncErrors.Add(new MlpMajorSyncError(offset, "checksum16 不符"));
            }
            var currentPeak = BinaryPrimitives.ReadUInt16BigEndian(majorSync[14..16]) & 0x7FFF;
            var currentExtendedInfo = majorSync[16] & 3;
            var currentSampleRate = SampleRateOf(majorSync);
            if (currentSampleRate is null)
            {
                majorSyncErrors.Add(new MlpMajorSyncError(offset, "采样率未知"));
            }
            else if (currentPeak < (long)BasePeakBitrate * 16 / currentSampleRate.Value ||
                     currentPeak > PeakBitrateRaw(currentSampleRate.Value))
            {
                majorSyncErrors.Add(new MlpMajorSyncError(
                    offset,
                    $"peak={currentPeak}，期望 {PeakBitrateRaw(currentSampleRate.Value)}"));
            }
            if (currentExtendedInfo != 1)
            {
                majorSyncErrors.Add(new MlpMajorSyncError(
                    offset, $"extended_substream_info={currentExtendedInfo}，期望 1"));
            }
            if (sampleRate is not null && currentSampleRate != sampleRate)
            {
                majorSyncErrors.Add(new MlpMajorSyncError(offset, "major sync 采样率不一致"));
            }
            if (peakRaw is not null && currentPeak != peakRaw)
            {
                majorSyncErrors.Add(new MlpMajorSyncError(offset, "major sync peak 字段不一致"));
            }
            if (extendedInfo is not null && currentExtendedInfo != extendedInfo)
            {
                majorSyncErrors.Add(new MlpMajorSyncError(offset, "major sync ext 字段不一致"));
            }
            peakRaw ??= currentPeak;
            extendedInfo ??= currentExtendedInfo;
            sampleRate ??= currentSampleRate;
        }

        foreach (var unit in units)
        {
            var headerOffset = unit.Offset + 4 + (unit.HasMajorSync ? MajorSyncSize : 0);
            if (headerOffset + 2 > unit.Offset + unit.Length)
            {
                substreamErrors.Add(unit.Offset);
                continue;
            }

            var substreamHeader = BinaryPrimitives.ReadUInt16BigEndian(data.Slice(headerOffset, 2));
            var end = (substreamHeader & 0x0FFF) * 2;
            var dataOffset = headerOffset + 2;
            var unitEnd = unit.Offset + unit.Length;
            if (end < 2 || dataOffset + end > unitEnd)
            {
                substreamErrors.Add(unit.Offset);
                continue;
            }
            var substreamData = data.Slice(dataOffset, end);

            var timing = BinaryPrimitives.ReadUInt16BigEndian(data.Slice(unit.Offset + 2, 2));
            var parity = timing ^ (unit.Length / 2);
            parity ^= (substreamHeader >> 8) & 0xFF;
            parity ^= substreamHeader & 0xFF;
            parity ^= parity >> 8;
            parity ^= parity >> 4;
            parity &= 0xF;
            var expected = (data[unit.Offset] >> 4) & 0xF;
            var actual = parity ^ 0xF;
            if (actual != expected)
            {
                parityErrors.Add(new MlpAuParityError(unit.Offset, expected, actual));
            }

            if (((substreamHeader >> 13) & 1) != 0)
            {
                if (substreamData.Length < 2)
                {
                    substreamErrors.Add(unit.Offset);
                    continue;
                }
                var body = substreamData[..^2];
                if (substreamData[^2] != (CalculateParity(body) ^ 0xA9) ||
                    substreamData[^1] != Checksum8(body, body.Length))
                {
                    substreamErrors.Add(unit.Offset);
                }
            }

            if (unit == units[^1] && substreamData.Length >= 6 &&
                substreamData[..^2].EndsWith(EndOfStreamBytes))
            {
                hasEndOfStream = true;
            }
        }

        return new MlpInspection(
            data.Length,
            units.Count,
            majorSyncOffsets.Length,
            majorSyncOffsets.Length == 0 ? 0 : (double)units.Count / majorSyncOffsets.Length,
            majorSyncErrors,
            parityErrors,
            substreamErrors,
            hasEndOfStream,
            peakRaw,
            extendedInfo,
            sampleRate);
    }

    public static MlpAlignmentResult Align(ReadOnlySpan<byte> data)
    {
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
