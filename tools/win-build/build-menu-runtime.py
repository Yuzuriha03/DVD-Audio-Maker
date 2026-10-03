"""Build only the C subpicture/AMGM modules used by the Windows x64 GUI."""
from pathlib import Path
import argparse, concurrent.futures, hashlib, json, os, subprocess, sys
sys.path.insert(0,str(Path(__file__).parent/'native'))
from pe_dependencies import Pe

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--msys-root',type=Path,default=Path('C:/msys64'))
    p.add_argument('--output',type=Path,default=Path('build/menu-native'))
    a=p.parse_args();root=Path(__file__).resolve().parents[1]/'menu-native';vendor=root/'vendor'
    out=a.output.resolve();out.mkdir(parents=True,exist_ok=True)
    compiler=a.msys_root.resolve()/'mingw64/bin/gcc.exe'
    env=os.environ|{'PATH':str(compiler.parent)+os.pathsep+os.environ['PATH']}
    flags=['-O2','-std=gnu11','-D_GNU_SOURCE','-ffunction-sections','-fdata-sections',
           '-I'+str(root),'-I'+str(vendor),'-include',str(root/'session.h')]
    groups={'spu':['subgen','subgen-parse-xml','subgen-encode','subgen-image','compat'],
            'nav':['dvdauthor','dvdcompile','dvdvml','dvdvmy','dvdifo','dvdvob','dvdpgc','dvdcli','compat']}
    files={}
    for group,names in groups.items():
        objects=out/('objects-'+group);objects.mkdir(exist_ok=True)
        sources=[vendor/(n+'.c') for n in names]+[root/'session.c',root/'readxml.c']
        group_flags=flags+(['-DDVDA_SUBPICTURE'] if group=='spu' else [])
        def compile_one(source):
            target=objects/(source.stem+'.o')
            local_flags=group_flags+(['-DDVDA_SESSION_IMPLEMENTATION'] if source.name=='session.c' else [])
            r=subprocess.run([str(compiler),*local_flags,'-c',str(source),'-o',str(target)],env=env,capture_output=True)
            log=objects/(source.stem+'.log');log.write_bytes(r.stdout+r.stderr)
            if r.returncode:raise RuntimeError('Compilation failed: '+str(log))
            return str(target)
        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            built=list(pool.map(compile_one,sources))
        dll=out/('dvda-menu-'+group+'.dll')
        cmd=[str(compiler),'-shared','-static-libgcc','-s','-Wl,--gc-sections','-Wl,--no-insert-timestamp',
             '-Wl,--exclude-all-symbols',*built,'-lxmllite','-lshlwapi','-lm','-o',str(dll)]
        with (out/(group+'-link.log')).open('wb') as log:
            subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
        imports=sorted(Pe(dll).imports())
        if any(not (Path(os.environ['SystemRoot'])/'System32'/n).exists() for n in imports):
            raise ValueError('Unexpected menu runtime import: '+str(imports))
        files[dll.name]={'bytes':dll.stat().st_size,'sha256':sha(dll),'imports':imports}
    verifier=out/'dvda-disc-verify.dll'
    subprocess.run([str(compiler),'-O2','-std=gnu11','-Wall','-Wextra','-Werror','-shared','-static-libgcc','-s',
                    '-Wl,--no-insert-timestamp','-Wl,--exclude-all-symbols',str(root/'disc-verify.c'),
                    '-o',str(verifier)],env=env,check=True)
    files[verifier.name]={'bytes':verifier.stat().st_size,'sha256':sha(verifier),'imports':sorted(Pe(verifier).imports())}
    record={'target':'Windows x64','scope':'DVD menu subpictures, DVD-Audio AMGM navigation and read-only MLP verification',
            'compiler':subprocess.check_output([str(compiler),'--version'],env=env).decode().splitlines()[0],
            'files':files,'source_inputs':{f.relative_to(root).as_posix():sha(f) for f in sorted(root.rglob('*')) if f.is_file()}}
    (out/'menu-build.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    (out/'NOTICE.txt').write_text('Menu algorithms derived from patched dvdauthor 0.7.1.\nSource: tools/menu-native/vendor; provenance: ORIGIN.json.\n\n'+(vendor/'COPYING').read_text(encoding='utf-8'),encoding='utf-8')
    print(json.dumps(files),flush=True)

if __name__=='__main__':main()
