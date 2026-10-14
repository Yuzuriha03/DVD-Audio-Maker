"""Negative controls for auxiliary-only MLP comparison."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('coded', Path(__file__).with_name('compare-mlp-coded.py'))
coded = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coded)


def unit(stamp=0, payload=b'abcdef', arrival=123, second=False, second_stamp=0):
    major = bytearray(28)
    major[:4] = bytes.fromhex('f8726fbb')
    major[16] = (2 if second else 1) << 4
    words = [0x2000 | stamp << 12 | 3]
    if second:
        words.append(0x2003 | second_stamp << 12)
    length = (4 + len(major) + 2 * len(words) + len(payload)) // 2
    header = length | (coded.parity(length, arrival, words) << 12)
    return header.to_bytes(2, 'big') + arrival.to_bytes(2, 'big') + major + b''.join(v.to_bytes(2, 'big') for v in words) + payload


class Comparison(unittest.TestCase):
    def test_only_stamp_and_derived_parity_are_excluded(self):
        with tempfile.TemporaryDirectory() as root:
            left, right = Path(root)/'left.mlp', Path(root)/'right.mlp'
            for second in (False, True):
                left.write_bytes(unit(second=second))
                right.write_bytes(unit(stamp=1, second=second))
                result = coded.compare(left, right)
                self.assertTrue(result['coded_equal'])
                self.assertFalse(result['file_equal'])
                self.assertEqual(left.read_bytes(), unit(second=second))
                self.assertEqual(right.read_bytes(), unit(stamp=1, second=second))
                right.write_bytes(unit(stamp=1, payload=b'abXdef', second=second))
                self.assertFalse(coded.compare(left, right)['coded_equal'])
                right.write_bytes(unit(stamp=1, arrival=124, second=second))
                self.assertFalse(coded.compare(left, right)['coded_equal'])
                if second:
                    right.write_bytes(unit(stamp=1, second=True, second_stamp=1))
                    self.assertFalse(coded.compare(left, right)['coded_equal'])

    def test_bad_parity_and_truncation_cannot_be_hidden(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root)/'bad.mlp'
            bad = bytearray(unit(stamp=1))
            bad[0] ^= 0x10
            for value in (bad, unit()[:-1], b'', b'123'):
                path.write_bytes(value)
                with self.assertRaises(ValueError):
                    coded.coded_hash(path)


if __name__ == '__main__':
    unittest.main()
