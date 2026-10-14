"""Hash complete MLP bytes except the first directory's auxiliary stamp bit.

This is NOT a file SHA256. Original AU header parity is verified first; only
the stamp bit and its dependent parity bit are changed in memory. This excludes
the entire auxiliary identification TLV stream (machine/license/application/time),
not any audio payload, AU size, arrival timestamp, major sync or other directory bits.
No PCM decoding is performed and neither input file is written.
"""
import argparse
import hashlib
import json
from pathlib import Path

SCHEME = 'mlp-coded-v1-no-auxiliary-stamp'


def parity(length_word, arrival, directories):
    value = (length_word & 0xFFF) ^ arrival
    for word in directories:
        value ^= word
    result = 15
    while value:
        result ^= value & 15
        value >>= 4
    return result


def coded_hash(path):
    original = hashlib.sha256()
    coded = hashlib.sha256()
    stamps = hashlib.sha256()
    size = units = 0
    substreams = None
    with Path(path).open('rb') as stream:
        while True:
            head = stream.read(4)
            if not head:
                break
            if len(head) != 4:
                raise ValueError(f'Truncated AU header at byte {size}')
            word = int.from_bytes(head[:2], 'big')
            length = (word & 0xFFF) * 2
            if length < 8:
                raise ValueError(f'Invalid AU length at byte {size}')
            body = stream.read(length - 4)
            if len(body) != length - 4:
                raise ValueError(f'Truncated AU at byte {size}')
            au = bytearray(head + body)
            original.update(au)
            major = au[4:8] == bytes.fromhex('f8726fbb')
            directory = 32 if major else 4
            if major:
                if len(au) < 34:
                    raise ValueError('Truncated major sync')
                substreams = au[20] >> 4
            if substreams not in (1, 2) or directory + substreams * 2 > len(au):
                raise ValueError('Missing or unsupported MLP major sync/directory')
            words = [int.from_bytes(au[directory + i * 2:directory + i * 2 + 2], 'big')
                     for i in range(substreams)]
            if any(v & 0x8000 for v in words):
                raise ValueError('Extra directory words are not supported by this comparison scheme')
            if word >> 12 != parity(word, int.from_bytes(head[2:], 'big'), words):
                raise ValueError(f'Invalid original AU header parity at byte {size}')
            stamp = (au[directory] >> 4) & 1
            stamps.update(bytes([stamp]))
            au[directory] &= 0xEF
            au[0] ^= stamp << 4
            coded.update(au)
            size += length
            units += 1
    if not units:
        raise ValueError('Empty MLP cannot pass comparison')
    return {'path': str(Path(path).resolve()), 'bytes': size, 'access_units': units,
            'file_sha256': original.hexdigest(), 'coded_sha256': coded.hexdigest(),
            'auxiliary_bits_sha256': stamps.hexdigest()}


def compare(reference, candidate):
    left, right = coded_hash(reference), coded_hash(candidate)
    return {'scheme': SCHEME, 'excluded': 'first substream auxiliary identification stream and its dependent header parity bit',
            'pcm_decoded': False, 'reference': left, 'candidate': right,
            'file_equal': left['file_sha256'] == right['file_sha256'] and left['bytes'] == right['bytes'],
            'coded_equal': left['coded_sha256'] == right['coded_sha256'] and left['bytes'] == right['bytes']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reference', type=Path)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    result = compare(args.reference, args.candidate)
    text = json.dumps(result, indent=2) + '\n'
    if args.report:
        with args.report.open('x', encoding='utf-8') as stream:
            stream.write(text)
    print(text)
    return 0 if result['coded_equal'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
