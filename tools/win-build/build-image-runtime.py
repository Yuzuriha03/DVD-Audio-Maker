"""Build a purpose-limited Windows x64 ImageMagick runtime in an isolated tree.

Does not alter the installed compiler or upstream dvda-author working tree.
"""
from pathlib import Path
import argparse, hashlib, json, os, shlex, subprocess, tarfile, urllib.request, shutil
import importlib.util

spec = importlib.util.spec_from_file_location('native_build', Path(__file__).with_name('build-minimal-ffmpeg.py'))
common = importlib.util.module_from_spec(spec)
spec.loader.exec_module(common)

def verified_sources(archive, work, source, patched):
    """Check every upstream file; rebuild only our explicit patch allowlist."""
    if not source.exists():
        with tarfile.open(archive) as tar: tar.extractall(work, filter='data')
    originals = {}
    with tarfile.open(archive) as tar:
        for member in tar.getmembers():
            if not member.isfile(): continue
            relative = Path(member.name).relative_to(source.name).as_posix()
            with tar.extractfile(member) as stream: original = stream.read()
            if relative in patched:
                originals[relative] = original.decode('utf-8')
            elif not (source/relative).is_file() or (source/relative).read_bytes() != original:
                raise ValueError('Unexpected upstream source modification: '+str(source/relative))
    if set(originals) != set(patched): raise ValueError('Patch source missing from pinned archive.')
    return originals

def write_changed(path, text):
    data = text.encode('utf-8')
    if not path.exists() or path.read_bytes() != data: path.write_bytes(data)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--msys-root', type=Path, required=True)
    parser.add_argument('--work-directory', type=Path, default=Path('build/imagemagick-minimal'))
    parser.add_argument('--prepare-only', action='store_true')
    parser.add_argument('--skip-configure', action='store_true')
    args = parser.parse_args()
    work = args.work_directory.resolve(); work.mkdir(parents=True, exist_ok=True)
    url = 'https://codeload.github.com/ImageMagick/ImageMagick/tar.gz/refs/tags/7.0.8-47'
    archive = work / 'ImageMagick-7.0.8-47.tar.gz'
    common.download(url, archive, '8d2bfa68fea5ca04b62c095da8c4707bb62cd1961a25d03162ac3112d42b346a')
    digest = common.sha(archive)
    source = work / 'ImageMagick-7.0.8-47'
    patched = ['coders/coders-list.h','MagickCore/magick-config.h','MagickCore/delegate.c',
        'coders/jpeg.c','coders/png.c','MagickCore/nt-base.c','coders/webp.c']
    originals = verified_sources(archive, work, source, patched)
    provenance = {'version':'7.0.8-47','source_url':url,'archive_sha256':digest,'target':'Windows x64'}
    (work/'source.json').write_text(json.dumps(provenance,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(provenance), flush=True)
    if args.prepare_only: return
    # Registration allowlist lets the linker discard unused static members.
    coders = ['JPEG','PNG','WEBP','XC','GRADIENT','INFO','NULL','RGB','GRAY','LABEL','CAPTION','MPR','MVG']
    write_changed(source/'coders/coders-list.h',''.join('AddMagickCoder('+name+')\n' for name in coders))
    text = originals['MagickCore/magick-config.h'].replace('#include "MagickCore/magick-baseconfig.h"',
        '#include "MagickCore/magick-baseconfig.h"\n/* DVDA: static coders only; 7.0.8 autodetects LTDL even without modules. */\n#undef MAGICKCORE_LTDL_DELEGATE')
    write_changed(source/'MagickCore/magick-config.h',text)
    text = originals['MagickCore/delegate.c']
    for signature,returned in [('MagickExport int ExternalDelegateCommand(', '-1'),('MagickExport MagickBooleanType InvokeDelegate(', 'MagickFalse')]:
        start=text.index('{',text.index(signature))+1
        text=text[:start]+'\n  /* DVDA: external delegate execution is not part of this runtime. */\n  (void) ThrowMagickException(exception,GetMagickModule(),PolicyError,"NotAuthorized","external delegates are disabled");\n  return '+returned+';\n'+text[start:]
    write_changed(source/'MagickCore/delegate.c',text)
    # Use matching UCRT setjmp/longjmp and the actual stack pointer. This keeps
    # codec errors/cancellation compatible with the protected .NET host stack.
    jump = ('/* DVDA: UCRT jump context supports protected .NET host stacks. */\n'
        '#include <setjmp.h>\n'
        '__declspec(dllimport) int __cdecl __attribute__((returns_twice)) __intrinsic_setjmp(jmp_buf, void *);\n'
        '#undef setjmp\n#define setjmp(buffer) __intrinsic_setjmp((buffer),mingw_getsp())\n')
    for name in ['png','jpeg']:
        relative = 'coders/'+name+'.c'
        text = '#define __USE_MINGW_SETJMP_NON_SEH 1\n'+originals[relative]
        if name == 'jpeg':
            text=text.replace('#include "jerror.h"','#include "jerror.h"\n/* DVDA: use the baseline JPEG API, not newer turbo lossless internals. */\n#if defined(LIBJPEG_TURBO_VERSION)\n#undef D_LOSSLESS_SUPPORTED\n#undef C_LOSSLESS_SUPPORTED\n#endif')
        index=text.index('\nstatic ')  # After all headers, which can redefine setjmp.
        write_changed(source/relative,text[:index]+jump+text[index:])
    text=originals['MagickCore/nt-base.c'].replace('  SetUnhandledExceptionFilter(NTUncaughtException);',
        '  /* DVDA: leave the embedding application exception handler intact. */')
    write_changed(source/'MagickCore/nt-base.c',text)
    text=originals['coders/webp.c'].replace('  entry->encoder=(EncodeImageHandler *) WriteWEBPImage;',
        '  /* DVDA: WebP covers are decoded; all generated images are JPEG/PNG. */')
    text=text.replace('WebPGetEncoderVersion()', 'WebPGetDecoderVersion()').replace('WEBP_ENCODER_ABI_VERSION);','WEBP_DECODER_ABI_VERSION);')
    write_changed(source/'coders/webp.c',text)
    build=work/'compile'; prefix=work/'install'
    build.mkdir(exist_ok=True); prefix.mkdir(exist_ok=True)
    disabled=['bzlib','zstd','x','dps','fftw','flif','fpx','djvu','fontconfig','raqm','gslib','gvc','heic','jbig','lcms','openjp2','lqr','lzma','openexr','pango','raw','rsvg','tiff','wmf','xml','autotrace','jemalloc','umem','perl','magick-plus-plus','utilities','modules']
    configure=[common.short_path(source)+'/configure','--prefix='+common.short_path(prefix),
        '--host=x86_64-w64-mingw32','--enable-static','--disable-shared','--disable-openmp','--disable-opencl',
        '--enable-hdri','--with-quantum-depth=16','--disable-docs',
        '--with-jpeg=yes','--with-png=yes','--with-webp=yes','--with-freetype=yes','--with-zlib=yes']
    configure+=['--without-'+name for name in disabled]
    commands=['set -eu','export PATH=/mingw64/bin:/usr/bin','export LC_ALL=C',
        'export PKG_CONFIG_PATH=/mingw64/lib/pkgconfig',
        "export CFLAGS='-O2 -std=gnu11 -ffunction-sections -fdata-sections -Wno-error=incompatible-pointer-types -Wno-error=implicit-function-declaration'",
        "export LDFLAGS='-Wl,--gc-sections -Wl,--no-insert-timestamp'",
        'cd '+shlex.quote(common.short_path(build))]
    if not args.skip_configure: commands.append(shlex.join(configure))
    commands.append(f'make -j{common.worker_count()} MagickCore/libMagickCore-7.Q16HDRI.la MagickWand/libMagickWand-7.Q16HDRI.la')
    script=work/'build.sh'; script.write_text('\n'.join(commands)+'\n',encoding='utf-8',newline='\n')
    msys=args.msys_root.resolve()
    env=os.environ|{'PATH':str(msys/'mingw64/bin')+os.pathsep+str(msys/'usr/bin')+os.pathsep+os.environ['PATH']}
    print('Compiling tailored x64 image libraries; see '+str(work/'build.log'),flush=True)
    with (work/'build.log').open('wb') as log:
        subprocess.run([str(msys/'usr/bin/bash.exe'),'--noprofile','--norc',common.posix(script)],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    provenance.update(configure=configure,coders=coders,compiler=subprocess.check_output([str(msys/'mingw64/bin/gcc.exe'),'--version'],env=env).decode().splitlines()[0])
    (work/'image-build.json').write_text(json.dumps(provenance,indent=2)+'\n',encoding='utf-8')
    print('Core and Wand static libraries ready.',flush=True)

if __name__ == '__main__': main()
