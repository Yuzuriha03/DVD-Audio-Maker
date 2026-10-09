"""Rebuild the configured Windows author in isolation with in-process images.

The source tree must be the already configured project fork. It is read only.
All consumed source files, headers, config and import libraries are hashed.
"""
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).parent))
from worker_policy import worker_count
import argparse, concurrent.futures, difflib, hashlib, json, os, re, shutil, subprocess

sys.path.insert(0, str(Path(__file__).parent / 'native'))
from pe_dependencies import Pe

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source',type=Path,required=True)
    p.add_argument('--msys-root',type=Path,required=True)
    p.add_argument('--work-directory',type=Path,default=Path('build/image-author'))
    p.add_argument('--menu-runtime',type=Path,default=Path('build/menu-native'))
    repo=Path(__file__).resolve().parents[2]
    default_ffmpeg=Path(os.environ.get('DVDA_FFMPEG_RUNTIME_DIR', str(repo/'build/ffmpeg-menu/install')))
    p.add_argument('--ffmpeg-runtime',type=Path,default=default_ffmpeg,
                   help='FFmpeg install prefix built by build-minimal-ffmpeg.py')
    a=p.parse_args(); source=a.source.resolve(); work=a.work_directory.resolve(); work.mkdir(parents=True,exist_ok=True)
    ffmpeg_runtime=a.ffmpeg_runtime.resolve()
    ffmpeg_lib=ffmpeg_runtime/'lib'
    ffmpeg_bin=ffmpeg_runtime/'bin'
    ffmpeg_imports={
        'avformat': ffmpeg_lib/'libavformat.dll.a',
        'avcodec': ffmpeg_lib/'libavcodec.dll.a',
        'avutil': ffmpeg_lib/'libavutil.dll.a',
    }
    ffmpeg_dlls=['avformat-63.dll','avcodec-63.dll','avutil-61.dll']
    ffmpeg_headers=[ffmpeg_runtime/'include/libavcodec/avcodec.h',
                    ffmpeg_runtime/'include/libavformat/avformat.h',
                    ffmpeg_runtime/'include/libavutil/avutil.h',
                    ffmpeg_runtime/'include/libavutil/audio_fifo.h',
                    ffmpeg_runtime/'include/libavutil/channel_layout.h',
                    ffmpeg_runtime/'include/libavutil/imgutils.h',
                    ffmpeg_runtime/'include/libavutil/opt.h',
                    ffmpeg_runtime/'include/libavutil/samplefmt.h']
    missing=[str(path) for path in [*ffmpeg_imports.values(), *(ffmpeg_bin/name for name in ffmpeg_dlls), *ffmpeg_headers] if not path.is_file()]
    if missing:
        raise FileNotFoundError('FFmpeg menu shared build is incomplete; run build-minimal-ffmpeg.py --profile menu first: '+', '.join(missing))
    snapshot=work/'source'; snapshot.mkdir(exist_ok=True)
    inputs={}
    for folder in ['src','libutils/src','libfixwav/src','local/include']:
        for file in (source/folder).rglob('*'):
            if file.is_file() and (file.suffix in ['.c','.h','.a','.rc','.manifest']):
                relative=file.relative_to(source); target=snapshot/relative; target.parent.mkdir(parents=True,exist_ok=True)
                shutil.copy2(file,target); inputs[relative.as_posix()]=sha(file)
    shutil.copy2(source/'config.h',snapshot/'config.h'); inputs['config.h']=sha(source/'config.h')
    for name, path in ffmpeg_imports.items():
        inputs[f'ffmpeg-runtime/lib/{path.name}']=sha(path)
    for name in ffmpeg_dlls:
        inputs[f'ffmpeg-runtime/bin/{name}']=sha(ffmpeg_bin/name)
    ffmpeg_manifest=ffmpeg_runtime/'build-manifest.json'
    if not ffmpeg_manifest.is_file():
        raise FileNotFoundError('FFmpeg menu profile manifest is missing: '+str(ffmpeg_manifest))
    manifest_data=json.loads(ffmpeg_manifest.read_text(encoding='utf-8'))
    if manifest_data.get('profile') not in ('menu', 'shared'):
        raise ValueError('The author requires the FFmpeg menu or shared profile, got: '+repr(manifest_data.get('profile')))
    inputs['ffmpeg-runtime/build-manifest.json']=sha(ffmpeg_manifest)
    # Keep the configured snapshot aligned with the repository's native
    # migration sources. The configured tree supplies generated headers,
    # libraries and menu data; these files carry the in-process ISO and menu
    # image paths.
    mirror_root=Path(__file__).resolve().parents[2] / 'tools/dvda-author-mlp8/src'
    for name in ['ats.c', 'audio.c', 'launch_manager.c', 'iso_writer.c', 'iso_writer.h', 'menu.c', 'amg2.c', 'command_line_parsing.c', 'auxiliary.c']:
        source_file=mirror_root/name
        target=snapshot/'src'/name
        shutil.copy2(source_file, target)
        inputs[f'project-mirror/src/{name}']=sha(source_file)
    patches=[]
    for name in ['menu.c','command_line_parsing.c']:
        path=snapshot/'src'/name; original=path.read_text('utf-8'); text=original
        for tool in ['convert','mogrify']:
            text=re.sub(rf'create_binary_path\({tool},\s*{tool.upper()},\s*SEPARATOR {tool.upper()}_BASENAME,\s*globals\)',f'strdup("{tool}")',text)
        variables=['command','courier->buf','command2','command1','command3'] if name=='menu.c' else ['cl']
        expected=6 if name=='menu.c' else 1
        replaced=0
        for variable in variables:
            old=f'system(win32quote({variable}))'; replaced+=text.count(old)
            text=text.replace(old,f'dvda_image_command({variable})')
        if replaced != expected and not (replaced == 0 and name in ('menu.c', 'command_line_parsing.c')):
            raise ValueError(f'Unexpected image call count in {name}: {replaced}')
        text='extern int dvda_image_command(const char *);\n'+text
        text=text.replace('rc == -1 || !WIFEXITED(rc) || WEXITSTATUS(rc) != 0','rc != 0')
        path.write_text(text,encoding='utf-8')
        patches.extend(difflib.unified_diff(original.splitlines(True),text.splitlines(True),fromfile='a/src/'+name,tofile='b/src/'+name))
    # Keep utility sources aligned with the repository mirror. Menu encoding
    # and navigation use the in-process APIs, not this legacy process helper.
    mirror_utils=Path(__file__).resolve().parents[2] / 'tools/dvda-author-mlp8/libutils/src/libc_utils.c'
    target_utils=snapshot/'libutils/src/libc_utils.c'
    shutil.copy2(mirror_utils, target_utils)
    inputs['project-mirror/libutils/src/libc_utils.c']=sha(mirror_utils)
    (work/'inprocess-images.patch').write_text(''.join(patches),encoding='utf-8')
    msys=a.msys_root.resolve(); compiler=str(msys/'mingw64/bin/gcc.exe')
    env=os.environ|{'PATH':str(msys/'mingw64/bin')+os.pathsep+os.environ['PATH']}
    include=[snapshot/'libutils/src/include',snapshot/'libutils/src/private',snapshot/'src/include',snapshot/'libfixwav/src/include',snapshot,ffmpeg_runtime/'include',snapshot/'local/include',Path(__file__).parent/'native']
    flags=['-O2','-std=gnu11','-D_GNU_SOURCE','-DHAVE_CONFIG_H','-DWITHOUT_sox','-DWITHOUT_FLAC','-DWITHOUT_libogg','-ffunction-sections','-fdata-sections','-Wno-error=incompatible-pointer-types','-Wno-error=implicit-function-declaration']+['-I'+str(x) for x in include]
    names='amg2 ats atsi2 audio auxiliary dvda-author file_input_parsing samg2 launch_manager command_line_parsing lexer ats2wav mlp menu asvs xml sound videoimport libsoxconvert iso_writer'.split()
    files=[snapshot/'src'/(name+'.c') for name in names]
    files+=list((snapshot/'libutils/src').glob('*.c'))+list((snapshot/'libfixwav/src').glob('*.c'))
    loader=Path(__file__).parent/'native/author-image-loader.c'; files.append(loader.resolve())
    inputs['project/author-image-loader.c']=sha(loader)
    menu_media=Path(__file__).parent/'native/dvda-menu-media.c'
    menu_header=Path(__file__).parent/'native/dvda-menu-media.h'
    files.append(menu_media.resolve())
    inputs['project/dvda-menu-media.c']=sha(menu_media)
    inputs['project/dvda-menu-media.h']=sha(menu_header)
    objects=work/'objects'; objects.mkdir(exist_ok=True)
    def compile_one(file):
        target=objects/(file.stem+'.o')
        result=subprocess.run([compiler,*flags,'-c',str(file),'-o',str(target)],env=env,capture_output=True)
        (objects/(file.stem+'.log')).write_bytes(result.stdout+result.stderr)
        if result.returncode: raise RuntimeError('Compilation failed; see '+str(objects/(file.stem+'.log')))
        return str(target)
    with concurrent.futures.ThreadPoolExecutor(max_workers=worker_count(len(files))) as pool: built=list(pool.map(compile_one,files))
    subprocess.run([str(msys/'mingw64/bin/windres.exe'),'-i','da-utf8.rc','-o',str(objects/'manifest.o')],cwd=snapshot/'src',env=env,check=True)
    # Override this link's default manifest through a private GCC specs file.
    # Never modify the compiler installation's default-manifest.o.
    specs=subprocess.check_output([compiler,'-dumpspecs'],env=env).decode()
    specs=re.sub(r'%\{!shared:%:if-exists\(default-manifest\.o%s\)\}', '', specs)
    specs_path=work/'gcc.specs'; specs_path.write_text(specs,encoding='utf-8')
    output=work/'dvda-author-dev.exe'
    # The MLP author imports only the three shared libraries produced by the
    # signed, source-built minimal FFmpeg profile.  The configured snapshot
    # still supplies generated/configuration headers. Its old static FFmpeg
    # archives are neither copied into the snapshot nor used by this link.
    libraries=[ffmpeg_imports['avformat'], ffmpeg_imports['avcodec'], ffmpeg_imports['avutil']]
    command=[compiler,'-specs='+str(specs_path),'-s','-static-libgcc','-Wl,--gc-sections','-Wl,--no-insert-timestamp',*built,str(objects/'manifest.o'),*[str(x) for x in libraries],'-lwinmm','-lbcrypt','-lm','-o',str(output)]
    with (work/'link.log').open('wb') as log: subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    if b'<activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>' not in output.read_bytes():
        raise ValueError('Missing UTF-8 process manifest')
    runtime_roots=[ffmpeg_bin, snapshot/'local/lib', repo/'build/media-native', repo/'build/ffmpeg-media/compile', msys/'mingw64/bin']
    system_root=Path(os.environ.get('SystemRoot', r'C:\Windows')).resolve()
    system_roots={system_root/'System32', system_root, msys/'bin', msys/'usr/bin'}
    system_names={'kernel32.dll','user32.dll','advapi32.dll','bcrypt.dll','combase.dll','gdi32.dll',
                  'gdi32full.dll','imm32.dll','msvcrt.dll','ntdll.dll','ole32.dll','oleaut32.dll',
                  'rpcrt4.dll','shell32.dll','shlwapi.dll','ucrtbase.dll','version.dll','winmm.dll',
                  'ws2_32.dll','wsock32.dll'}

    def is_system_import(name):
        lowered=name.lower()
        return lowered in system_names or lowered.startswith(('api-ms-win-', 'ext-ms-win-')) or any((root/name).exists() for root in system_roots)

    def locate(name):
        lowered=name.lower()
        for root in runtime_roots:
            direct=root/name
            if direct.is_file():
                return direct
            for candidate in root.rglob('*'):
                if candidate.is_file() and candidate.name.lower() == lowered:
                    return candidate
        return None

    # The work directory is dedicated to this build; remove runtime DLLs from
    # an older manifest before recording the new import closure.
    for stale in work.glob('*.dll'):
        stale.unlink()
    pending=[output]
    copied=set()
    while pending:
        file=pending.pop()
        for name in Pe(file).imports():
            destination=work/name
            if destination.is_file():
                if name not in copied:
                    copied.add(name); pending.append(destination)
                continue
            if is_system_import(name):
                continue
            candidate=locate(name)
            if candidate is None:
                raise FileNotFoundError('Unresolved author runtime dependency: '+name)
            if candidate.resolve() != destination.resolve():
                shutil.copy2(candidate, destination)
            copied.add(name)
            pending.append(destination)
    menu_runtime=a.menu_runtime.resolve()
    menu_manifest=json.loads((menu_runtime/'menu-build.json').read_text(encoding='utf-8'))
    for name in ['dvda-menu-spu.dll','dvda-menu-nav.dll']:
        path=menu_runtime/name
        if sha(path)!=menu_manifest['files'][name]['sha256']:
            raise ValueError('Menu library checksum mismatch: '+name)
        shutil.copy2(path,work/name)
    shutil.copy2(menu_runtime/'menu-build.json',work/'menu-build.json')
    shutil.copy2(menu_runtime/'NOTICE.txt',work/'menu-NOTICE.txt')
    inputs['menu-runtime/menu-build.json']=sha(menu_runtime/'menu-build.json')
    inputs['project/menu-api.h']=sha(repo/'tools/menu-native/menu-api.h')
    runtime_files={p.name:{'sha256':sha(p),'bytes':p.stat().st_size,'imports':sorted(Pe(p).imports())}
                   for p in sorted(work.glob('*.dll'))}
    profile=manifest_data['profile']
    record={'target':'Windows x64','menu_linkage':'in-process-source-built','ffmpeg_linkage':'shared-source-built-'+profile+'-profile',
            'ffmpeg_profile':'build-minimal-ffmpeg.py:'+profile,'files':{output.name:{'sha256':sha(output),'bytes':output.stat().st_size}},
            'runtime_files':runtime_files,'source_inputs':inputs,'patch_sha256':sha(work/'inprocess-images.patch'),
            'compiler':subprocess.check_output([compiler,'--version'],env=env).decode().splitlines()[0]}
    (work/'author-build.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(record['files']),flush=True)

if __name__=='__main__':main()
