"""Build the Rust Windows x64 media adapter and authenticated runtime inventory."""
from pathlib import Path
import argparse, subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--msys-root', type=Path, required=True)
    parser.add_argument('--prefix', type=Path, default=Path('build/ffmpeg-media/install'))
    parser.add_argument('--output', type=Path, default=Path('build/media-native'))
    args = parser.parse_args()
    subprocess.run(['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
        str(Path(__file__).with_name('build-rust-bridges.ps1')), '-Component', 'media',
        '-MsysRoot', str(args.msys_root.resolve()), '-FfmpegPrefix', str(args.prefix.resolve()),
        '-Output', str(args.output.resolve())], check=True)


if __name__ == '__main__':
    main()
