"""Compatibility entry point for the MSVC-only FFmpeg source builder.

The former GNU/MinGW builder has been replaced by build-msvc-ffmpeg.py.
MSYS2 is used only for configure, make, and signature verification.
"""
from pathlib import Path
import importlib.util
import sys

sys.path.insert(0, str(Path(__file__).parent))
spec = importlib.util.spec_from_file_location(
    'dvda_msvc_ffmpeg', Path(__file__).with_name('build-msvc-ffmpeg.py'))
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)

# Historical image-maintenance helpers import these archive/path functions.
download = builder.download
sha = builder.sha
posix = builder.posix
short_path = builder.short_path
worker_count = builder.worker_count
main = builder.main

if __name__ == '__main__':
    main()
