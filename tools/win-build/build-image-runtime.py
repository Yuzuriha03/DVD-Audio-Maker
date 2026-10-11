"""MSVC-only compatibility entry point for build-msvc-image.py."""
import argparse
from pathlib import Path
import subprocess
import sys

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--msys-root', help=argparse.SUPPRESS)
    parser.add_argument('--work-directory', type=Path, default=Path('build/msvc-native/image'))
    parser.add_argument('--source-directory', type=Path)
    parser.add_argument('--cache-directory', type=Path)
    parser.add_argument('--jobs', type=int)
    parser.add_argument('--prepare-only', action='store_true')
    parser.add_argument('--skip-configure', action='store_true')
    args = parser.parse_args()
    cmd = [sys.executable, str(Path(__file__).with_name('build-msvc-image.py')), '--work-directory', str(args.work_directory)]
    for key in ('source_directory', 'cache_directory', 'jobs'):
        value = getattr(args, key)
        if value is not None: cmd.extend(['--'+key.replace('_', '-'), str(value)])
    if args.prepare_only: cmd.append('--prepare-only')
    subprocess.run(cmd, check=True)

if __name__ == '__main__': main()
