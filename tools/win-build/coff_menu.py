"""Namespace MSVC COFF symbols and expose complete writable vendor sections.

Appending symbols keeps every relocation and auxiliary symbol index unchanged.
The initial string table is preserved so long section names remain valid too.
"""
import struct
from pathlib import Path


class Coff:
    def __init__(self, path):
        self.path = Path(path)
        self.data = bytearray(self.path.read_bytes())
        if len(self.data) < 20:
            raise ValueError('Truncated COFF object: '+str(path))
        machine, count, _, pointer, symbols, optional, _ = struct.unpack_from('<HHIIIHH', self.data)
        if machine != 0x8664 or optional:
            raise ValueError('Expected ordinary MSVC x64 COFF object: '+str(path))
        self.pointer, self.count = pointer, symbols
        self.sections = []
        for index in range(count):
            offset = 20+40*index
            name = bytes(self.data[offset:offset+8]).split(b'\0')[0].decode('ascii')
            virtual_size, _, raw_size = struct.unpack_from('<III', self.data, offset+8)
            flags = struct.unpack_from('<I', self.data, offset+36)[0]
            self.sections.append({'index': index+1, 'name': name,
                                  'bytes': max(virtual_size, raw_size), 'flags': flags})
        end = pointer+18*symbols
        string_size = struct.unpack_from('<I', self.data, end)[0]
        self.strings = bytearray(self.data[end:end+string_size])
        self.records = bytearray(self.data[pointer:end])

    def symbols(self):
        index = 0
        while index < self.count:
            offset = index*18
            name = bytes(self.records[offset:offset+8])
            if name[:4] == b'\0'*4:
                start = struct.unpack_from('<I', name, 4)[0]
                name = bytes(self.strings[start:]).split(b'\0')[0]
            else:
                name = name.split(b'\0')[0]
            value, section, kind, storage, auxiliaries = struct.unpack_from('<IhHBB', self.records, offset+8)
            yield index, name.decode('ascii'), value, section, kind, storage
            index += 1+auxiliaries

    def definitions(self):
        return {name for _, name, _, section, _, storage in self.symbols()
                if storage == 2 and section > 0}

    def undefined(self):
        return {name for _, name, value, section, _, storage in self.symbols()
                if storage == 2 and section == 0 and value == 0}

    def encoded_name(self, name):
        encoded = name.encode('ascii')
        if len(encoded) <= 8:
            return encoded.ljust(8, b'\0')
        offset = len(self.strings)
        self.strings.extend(encoded+b'\0')
        return b'\0'*4+struct.pack('<I', offset)

    def rewrite(self, mapping, state_prefix=None):
        for index, name, _, _, _, storage in list(self.symbols()):
            if storage == 2 and name in mapping:
                self.records[index*18:index*18+8] = self.encoded_name(mapping[name])
        states = []
        if state_prefix:
            for section in self.sections:
                if not section['bytes'] or not section['flags'] & 0x80000000:
                    continue
                # Never snapshot COMDAT data: it can be discarded or folded by
                # the linker, so one alias cannot safely describe its lifetime.
                if section['flags'] & 0x1000:
                    raise ValueError('Writable COMDAT requires /Gw-: '+str(self.path))
                name = state_prefix+'_'+str(len(states))
                self.records.extend(self.encoded_name(name)+struct.pack('<IhHBB', 0, section['index'], 0, 2, 0))
                self.count += 1
                states.append({'symbol': name, 'bytes': section['bytes'],
                               'object': self.path.name, 'section': section['name']})
        struct.pack_into('<I', self.strings, 0, len(self.strings))
        struct.pack_into('<I', self.data, 12, self.count)
        self.path.write_bytes(self.data[:self.pointer]+self.records+self.strings)
        return states
