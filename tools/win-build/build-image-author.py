"""Rebuild the configured Windows author in isolation with in-process images.

The source tree must be the already configured project fork. It is read only.
All consumed source files, headers, config and import libraries are hashed.
"""
from pathlib import Path
import argparse, concurrent.futures, difflib, hashlib, json, os, re, shutil, subprocess

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source',type=Path,required=True)
    p.add_argument('--msys-root',type=Path,required=True)
    p.add_argument('--work-directory',type=Path,default=Path('build/image-author'))
    a=p.parse_args(); source=a.source.resolve(); work=a.work_directory.resolve(); work.mkdir(parents=True,exist_ok=True)
    snapshot=work/'source'; snapshot.mkdir(exist_ok=True)
    inputs={}
    for folder in ['src','libutils/src','libfixwav/src','local/include','local/lib']:
        for file in (source/folder).rglob('*'):
            if file.is_file() and (file.suffix in ['.c','.h','.a','.rc','.manifest']):
                relative=file.relative_to(source); target=snapshot/relative; target.parent.mkdir(parents=True,exist_ok=True)
                shutil.copy2(file,target); inputs[relative.as_posix()]=sha(file)
    shutil.copy2(source/'config.h',snapshot/'config.h'); inputs['config.h']=sha(source/'config.h')
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
        if replaced!=expected: raise ValueError(f'Unexpected image call count in {name}: {replaced}')
        text='extern int dvda_image_command(const char *);\n'+text
        text=text.replace('rc == -1 || !WIFEXITED(rc) || WEXITSTATUS(rc) != 0','rc != 0')
        path.write_text(text,encoding='utf-8')
        patches.extend(difflib.unified_diff(original.splitlines(True),text.splitlines(True),fromfile='a/src/'+name,tofile='b/src/'+name))
    (work/'inprocess-images.patch').write_text(''.join(patches),encoding='utf-8')
    msys=a.msys_root.resolve(); compiler=str(msys/'mingw64/bin/gcc.exe')
    env=os.environ|{'PATH':str(msys/'mingw64/bin')+os.pathsep+os.environ['PATH']}
    include=[snapshot/'libutils/src/include',snapshot/'libutils/src/private',snapshot/'src/include',snapshot/'libfixwav/src/include',snapshot,snapshot/'local/include']
    flags=['-O2','-std=gnu11','-D_GNU_SOURCE','-DHAVE_CONFIG_H','-DWITHOUT_sox','-DWITHOUT_FLAC','-DWITHOUT_libogg','-ffunction-sections','-fdata-sections','-Wno-error=incompatible-pointer-types','-Wno-error=implicit-function-declaration']+['-I'+str(x) for x in include]
    names='amg2 ats atsi2 audio auxiliary dvda-author file_input_parsing samg2 launch_manager command_line_parsing lexer ats2wav mlp menu asvs xml sound videoimport libsoxconvert'.split()
    files=[snapshot/'src'/(name+'.c') for name in names]
    files+=list((snapshot/'libutils/src').glob('*.c'))+list((snapshot/'libfixwav/src').glob('*.c'))
    loader=Path(__file__).parent/'native/author-image-loader.c'; files.append(loader.resolve())
    inputs['project/author-image-loader.c']=sha(loader)
    objects=work/'objects'; objects.mkdir(exist_ok=True)
    def compile_one(file):
        target=objects/(file.stem+'.o')
        result=subprocess.run([compiler,*flags,'-c',str(file),'-o',str(target)],env=env,capture_output=True)
        (objects/(file.stem+'.log')).write_bytes(result.stdout+result.stderr)
        if result.returncode: raise RuntimeError('Compilation failed; see '+str(objects/(file.stem+'.log')))
        return str(target)
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool: built=list(pool.map(compile_one,files))
    subprocess.run([str(msys/'mingw64/bin/windres.exe'),'-i','da-utf8.rc','-o',str(objects/'manifest.o')],cwd=snapshot/'src',env=env,check=True)
    # Override this link's default manifest through a private GCC specs file.
    # Never modify the compiler installation's default-manifest.o.
    specs=subprocess.check_output([compiler,'-dumpspecs'],env=env).decode()
    specs=re.sub(r'%\{!shared:%:if-exists\(default-manifest\.o%s\)\}', '', specs)
    specs_path=work/'gcc.specs'; specs_path.write_text(specs,encoding='utf-8')
    output=work/'dvda-author-dev.exe'
    libraries=[snapshot/'local/lib'/('lib'+name+'.a') for name in ['swresample','avformat','avfilter','avcodec','avutil']]
    command=[compiler,'-specs='+str(specs_path),'-s','-static-libgcc','-Wl,--gc-sections','-Wl,--no-insert-timestamp',*built,str(objects/'manifest.o'),*[str(x) for x in libraries],'-lwinmm','-lbcrypt','-lm','-o',str(output)]
    with (work/'link.log').open('wb') as log: subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    if b'<activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>' not in output.read_bytes():
        raise ValueError('Missing UTF-8 process manifest')
    record={'target':'Windows x64','files':{output.name:{'sha256':sha(output),'bytes':output.stat().st_size}},'source_inputs':inputs,'patch_sha256':sha(work/'inprocess-images.patch'),'compiler':subprocess.check_output([compiler,'--version'],env=env).decode().splitlines()[0]}
    (work/'author-build.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(record['files']),flush=True)

if __name__=='__main__':main()
