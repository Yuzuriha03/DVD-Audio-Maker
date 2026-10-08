"""Link the tailored static image libraries into one Windows x64 runtime DLL."""
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).parent))
from worker_policy import worker_count
import argparse, concurrent.futures, importlib.util, json, os, re, subprocess, tarfile

spec=importlib.util.spec_from_file_location('common',Path(__file__).with_name('build-minimal-ffmpeg.py'))
common=importlib.util.module_from_spec(spec); spec.loader.exec_module(common)
spec=importlib.util.spec_from_file_location('image_source',Path(__file__).with_name('build-image-runtime.py'))
image_source=importlib.util.module_from_spec(spec);spec.loader.exec_module(image_source)

def build_freetype(work,compiler,ar,env):
    archive=work/'freetype-VER-2-10-0.tar.gz'
    url='https://codeload.github.com/freetype/freetype/tar.gz/refs/tags/VER-2-10-0'
    digest='9da0dbd24e7519f4f84ab2e5b203378e5a7d81cc10250ad52d0e065ad611d0cf'
    common.download(url,archive,digest)
    source=work/'freetype-VER-2-10-0'
    originals=image_source.verified_sources(archive,work,source,[
        'include/freetype/config/ftmodule.h','include/freetype/config/ftoption.h','src/base/ftsystem.c'])
    # TrueType/OpenType/TTC outlines used by the bundled and user-selected fonts.
    # Exclude bitmap/PostScript-only formats, compressed fonts and shaping engines.
    modules=['autofit_module_class','tt_driver_class','cff_driver_class','psaux_module_class',
             'psnames_module_class','pshinter_module_class','ft_raster1_renderer_class','sfnt_module_class',
             'ft_smooth_renderer_class','ft_smooth_lcd_renderer_class','ft_smooth_lcdv_renderer_class']
    header=source/'include/freetype/config/ftmodule.h'
    header.write_text('\n'.join(line for line in originals['include/freetype/config/ftmodule.h'].splitlines()
        if line.startswith('FT_USE_MODULE') and any(re.search(r'\b'+name+r'\b',line) for name in modules))+'\n')
    options=source/'include/freetype/config/ftoption.h'; text=originals['include/freetype/config/ftoption.h']
    text=text.replace('#define FT_CONFIG_OPTION_USE_LZW','#undef FT_CONFIG_OPTION_USE_LZW')
    text=text.replace('#define FT_CONFIG_OPTION_USE_ZLIB','#undef FT_CONFIG_OPTION_USE_ZLIB'); options.write_text(text)
    system=source/'src/base/ftsystem.c';text=originals['src/base/ftsystem.c']
    if 'dvda_utf8_fopen' not in text:
        helper='''
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
static FT_FILE *dvda_utf8_fopen(const char *path)
{
  int count=MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,path,-1,NULL,0);
  if (!count) return NULL;
  wchar_t *wide=malloc((size_t)count*sizeof(wchar_t));
  if (!wide) return NULL;
  MultiByteToWideChar(CP_UTF8,0,path,-1,wide,count);
  FT_FILE *file=_wfopen(wide,L"rb"); free(wide); return file;
}
'''
        text=text.replace('#include FT_TYPES_H','#include FT_TYPES_H\n'+helper)
        text=text.replace('ft_fopen( filepathname, "rb" )','dvda_utf8_fopen( filepathname )');system.write_text(text)
    names=['base/'+name for name in ['ftbase','ftbbox','ftbitmap','ftdebug','ftglyph','ftinit','ftmm','ftstroke','ftsynth','ftsystem']]
    names+=['autofit/autofit','truetype/truetype','cff/cff','psaux/psaux','psnames/psnames','pshinter/pshinter','raster/raster','sfnt/sfnt','smooth/smooth']
    obj=work/'freetype-objects';obj.mkdir(exist_ok=True)
    def compile_one(name):
        target=obj/(Path(name).name+'.o')
        command=[compiler,'-O2','-std=gnu11','-DFT2_BUILD_LIBRARY','-ffunction-sections','-fdata-sections','-I'+str(source/'include'),'-c',str(source/'src'/(name+'.c')),'-o',str(target)]
        p=subprocess.run(command,env=env,capture_output=True)
        (obj/(Path(name).name+'.log')).write_bytes(p.stdout+p.stderr)
        if p.returncode:raise RuntimeError('FreeType compilation: '+name)
        return str(target)
    with concurrent.futures.ThreadPoolExecutor(max_workers=worker_count(len(names))) as pool:objects=list(pool.map(compile_one,names))
    lib=work/'libfreetype-minimal.a'
    if lib.exists():lib.unlink()
    subprocess.run([ar,'rcs',str(lib),*objects],env=env,check=True)
    return lib,{'version':'2.10.0','source_url':url,'archive_sha256':digest,'modules':modules,'options_sha256':common.sha(options),'modules_sha256':common.sha(header),'utf8_io_sha256':common.sha(system)}

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--msys-root',required=True,type=Path)
    p.add_argument('--work-directory',type=Path,default=Path('build/imagemagick-minimal'))
    p.add_argument('--output',type=Path,default=Path('build/image-native'))
    p.add_argument('--freetype-only',action='store_true')
    a=p.parse_args();msys=a.msys_root.resolve();work=a.work_directory.resolve();output=a.output.resolve();output.mkdir(parents=True,exist_ok=True)
    compiler=str(msys/'mingw64/bin/gcc.exe');env=os.environ|{'PATH':str(msys/'mingw64/bin')+os.pathsep+os.environ['PATH']}
    ft,ftmeta=build_freetype(work,compiler,str(msys/'mingw64/bin/ar.exe'),env)
    if a.freetype_only:print('Minimal FreeType ready.');return
    source=work/'ImageMagick-7.0.8-47';build=work/'compile'
    bridge=Path(__file__).parent/'native/dvda-image.c'
    deps=[msys/'mingw64/lib'/name for name in ['libjpeg.a','libpng16.a','libwebpdecoder.a','libwebpmux.a','libz.a']]
    libraries=[build/'MagickWand/.libs/libMagickWand-7.Q16HDRI.a',build/'MagickCore/.libs/libMagickCore-7.Q16HDRI.a',ft,*deps]
    jump_def=work/'ucrt-jump.def';jump_lib=work/'libucrt-jump.a'
    jump_def.write_text('LIBRARY ucrtbase.dll\nEXPORTS\n__intrinsic_setjmp\nlongjmp\n')
    subprocess.run([str(msys/'mingw64/bin/dlltool.exe'),'-d',str(jump_def),'-l',str(jump_lib),'-m','i386:x86-64'],env=env,check=True)
    libraries.append(jump_lib)
    dll=output/'dvda-image.dll'
    command=[compiler,'-shared','-O2','-std=gnu11','-D_LIB','-D_MT','-DMAGICKCORE_HDRI_ENABLE=1','-DMAGICKCORE_QUANTUM_DEPTH=16',
        '-I'+str(build),'-I'+str(source),'-ffunction-sections','-fdata-sections','-static','-static-libgcc','-s',
        '-Wl,--gc-sections','-Wl,--no-insert-timestamp','-Wl,--exclude-all-symbols',str(bridge.resolve()),
        '-Wl,--start-group',*[str(x) for x in libraries],'-Wl,--end-group','-lshell32','-lgdi32','-lws2_32','-lm','-o',str(dll)]
    with (work/'bridge.log').open('wb') as log:subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    imports=subprocess.check_output([str(msys/'mingw64/bin/objdump.exe'),'-p',str(dll)],env=env).decode()
    imports=re.findall(r'DLL Name:\s*(\S+)',imports)
    if any(not (Path(os.environ['SystemRoot'])/'System32'/name).exists() for name in imports):raise ValueError('Unexpected non-system import: '+str(imports))
    manifest=json.loads((work/'image-build.json').read_text())
    manifest.update(freetype=ftmeta,bridge_sha256=common.sha(bridge),static_dependencies={p.name:common.sha(p) for p in deps},
        source_patches={name:common.sha(source/name) for name in ['coders/coders-list.h','MagickCore/delegate.c','coders/jpeg.c','coders/png.c','coders/webp.c','MagickCore/magick-config.h','MagickCore/nt-base.c']},
        files={dll.name:{'sha256':common.sha(dll),'bytes':dll.stat().st_size,'imports':imports}})
    (output/'image-build.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
    (output/'policy.xml').write_text('<policymap>\n  <policy domain="delegate" rights="none" pattern="*"/>\n</policymap>\n',encoding='utf-8')
    (output/'colors.xml').write_text('<colormap/>\n',encoding='utf-8')
    notices=[('ImageMagick',source/'LICENSE'),('FreeType',work/'freetype-VER-2-10-0/docs/FTL.TXT')]
    for component in ['libjpeg-turbo','libpng','libwebp','libwinpthread','zlib']:
        notices += [(component,path) for path in sorted((msys/'mingw64/share/licenses'/component).glob('*')) if path.is_file()]
    (output/'NOTICE.txt').write_text('\n\n'.join('=== '+name+' / '+path.name+' ===\n'+path.read_text('utf-8',errors='replace')
        for name,path in notices),encoding='utf-8')
    print(json.dumps(manifest['files']),flush=True)

if __name__=='__main__':main()
