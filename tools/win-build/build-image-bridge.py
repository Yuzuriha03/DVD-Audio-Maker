"""MSVC-only Rust image adapter entry point."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def authenticate(work):
    record=json.loads((work/'image-build.json').read_text(encoding='utf-8'))
    if record.get('target')!='x86_64-pc-windows-msvc' or record.get('toolchain')!='msvc' or record.get('crt')!='static':
        raise ValueError('MSVC static image manifest required')
    for name,item in record['files'].items():
        if sha(work/name)!=item['sha256']: raise ValueError('Image library checksum mismatch: '+name)
    return record
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--msys-root',help=argparse.SUPPRESS)
    p.add_argument('--work-directory',type=Path,default=Path('build/msvc-native/image'));p.add_argument('--output',type=Path,default=Path('build/rust-image-msvc'))
    p.add_argument('--source-directory',type=Path);p.add_argument('--cache-directory',type=Path);p.add_argument('--jobs',type=int);p.add_argument('--skip-native-build',action='store_true');p.add_argument('--freetype-only',action='store_true');a=p.parse_args()
    work=a.work_directory.resolve()
    if not a.skip_native_build:
        cmd=[sys.executable,str(Path(__file__).with_name('build-msvc-image.py')),'--work-directory',str(work)]
        for key in ('source_directory','cache_directory','jobs'):
            value=getattr(a,key)
            if value is not None: cmd.extend(['--'+key.replace('_','-'),str(value)])
        subprocess.run(cmd,check=True)
    authenticate(work)
    if a.freetype_only: return
    subprocess.run(['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(Path(__file__).with_name('build-rust-bridges.ps1')),'-Component','image','-MagickWork',str(work),'-ImageDependencies',str(work/'lib'),'-ImageOracle',str(work),'-Output',str(a.output.resolve())],check=True)
if __name__=='__main__': main()
