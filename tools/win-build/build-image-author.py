"""Compatibility entrypoint for the complete Rust DVD-Audio author builder.

All arguments are handled by build-rust-author.py. The legacy C producer has
been retired; configured source directories only supply optional menu assets.
"""
from pathlib import Path
import runpy


def main():
    runpy.run_path(str(Path(__file__).with_name('build-rust-author.py')), run_name='__main__')


if __name__ == '__main__':
    main()
