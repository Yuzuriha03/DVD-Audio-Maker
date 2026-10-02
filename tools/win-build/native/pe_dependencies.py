"""Read native PE imports/exports without installing a Python package."""
from pathlib import Path
import struct


class Pe:
    def __init__(self, path):
        self.path = Path(path)
        self.data = self.path.read_bytes()
        self.pe = self.u32(0x3c)
        if self.data[:2] != b'MZ' or self.data[self.pe:self.pe + 4] != b'PE\0\0':
            raise ValueError('Not a PE: ' + str(path))
        self.machine = self.u16(self.pe + 4)
        optional = self.pe + 24
        self.is64 = self.u16(optional) == 0x20b
        self.pointer_size = 8 if self.is64 else 4
        self.base = self.integer(optional + (24 if self.is64 else 28), self.pointer_size)
        self.dirs = optional + (112 if self.is64 else 96)
        section_start = optional + self.u16(self.pe + 20)
        self.sections = [struct.unpack_from('<IIII', self.data, section_start + i * 40 + 8)
                         for i in range(self.u16(self.pe + 6))]
        self.headers_size = self.u32(optional + 60)

    def integer(self, offset, size):
        return struct.unpack_from({2: '<H', 4: '<I', 8: '<Q'}[size], self.data, offset)[0]

    def u16(self, offset):
        return self.integer(offset, 2)

    def u32(self, offset):
        return self.integer(offset, 4)

    def offset(self, rva):
        for virtual_size, start, raw_size, raw in self.sections:
            if start <= rva < start + max(virtual_size, raw_size):
                return raw + rva - start
        if rva < self.headers_size:
            return rva
        raise ValueError(f'Invalid RVA {rva:x}: {self.path}')

    def string(self, rva):
        start = self.offset(rva)
        return self.data[start:self.data.index(0, start)].decode('ascii')

    def imports(self):
        result = {}
        for index, record_size, delayed in [(1, 20, False), (13, 32, True)]:
            rva = self.u32(self.dirs + 8 * index)
            if not rva:
                continue
            record = self.offset(rva)
            while any(self.data[record:record + record_size]):
                adjust = self.base if delayed and not self.u32(record) & 1 else 0
                name = self.u32(record + (4 if delayed else 12)) - adjust
                thunk = self.u32(record + (16 if delayed else 0))
                if not thunk:
                    thunk = self.u32(record + (12 if delayed else 16))
                thunk -= adjust
                offset = self.offset(thunk)
                symbols = set()
                while (value := self.integer(offset, self.pointer_size)):
                    ordinal_bit = 1 << (self.pointer_size * 8 - 1)
                    symbols.add('#' + str(value & 0xffff) if value & ordinal_bit else self.string(value - adjust + 2))
                    offset += self.pointer_size
                result.setdefault(self.string(name).lower(), set()).update(symbols)
                record += record_size
        return result

    def exports(self):
        rva = self.u32(self.dirs)
        if not rva:
            return set()
        directory = self.offset(rva)
        first_ordinal = self.u32(directory + 16)
        function_count = self.u32(directory + 20)
        count = self.u32(directory + 24)
        names = self.offset(self.u32(directory + 32))
        return {self.string(self.u32(names + i * 4)) for i in range(count)} | {
            '#' + str(first_ordinal + i) for i in range(function_count)}
