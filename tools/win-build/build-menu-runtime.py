"""Build the vendor menu algorithms with the Rust XML/resource boundary."""
from pathlib import Path
import argparse, concurrent.futures, hashlib, json, os, subprocess, sys
sys.path.insert(0, str(Path(__file__).parent))
from worker_policy import worker_count
sys.path.insert(0,str(Path(__file__).parent/'native'))
from pe_dependencies import Pe

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--msys-root',type=Path,default=Path('C:/msys64'))
    p.add_argument('--direct-link', action='store_true', help='Build namespaced vendor archives only (no owned DLL or Cargo recursion)')
    p.add_argument('--output',type=Path,default=Path('build/menu-native'))
    a=p.parse_args();root=Path(__file__).resolve().parents[1]/'menu-native';vendor=root/'vendor'
    repo=root.parents[1]
    cargo=Path.home()/'.cargo/bin/cargo.exe'
    rustc_version=subprocess.check_output([str(cargo.with_name('rustc.exe')),'--version']).decode().strip()
    crate=repo/'rust/crates/dvda-menu'
    inputs=[repo/'rust/Cargo.toml',repo/'rust/Cargo.lock',Path(__file__).resolve(),*sorted(crate.rglob('*.rs')),crate/'Cargo.toml']
    rust_inputs={f.relative_to(repo).as_posix():sha(f) for f in inputs}
    if not a.direct_link:
        subprocess.run([str(cargo),'build','--locked','--manifest-path',str(repo/'rust/Cargo.toml'),'-p','dvda-menu',
                        '--release','--target','x86_64-pc-windows-gnu'],check=True)
    archive=repo/'rust/target/x86_64-pc-windows-gnu/release/libdvda_menu.a'
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
        sources=[vendor/(n+'.c') for n in names]+[root/'session-rust.c']
        group_flags=flags+(['-DDVDA_SUBPICTURE'] if group=='spu' else [])+(['-DDVDA_DIRECT_LINK'] if a.direct_link else [])
        def compile_one(source):
            target=objects/(source.stem+'.o')
            local_flags=group_flags+(['-DDVDA_SESSION_IMPLEMENTATION'] if source.name == 'session-rust.c' else [])
            r=subprocess.run([str(compiler),*local_flags,'-c',str(source),'-o',str(target)],env=env,capture_output=True)
            log=objects/(source.stem+'.log');log.write_bytes(r.stdout+r.stderr)
            if r.returncode:raise RuntimeError('Compilation failed: '+str(log))
            return str(target)
        with concurrent.futures.ThreadPoolExecutor(max_workers=worker_count(len(sources))) as pool:
            built=list(pool.map(compile_one,sources))
        if a.direct_link:
            # Namespace every vendor definition; imported CRT/Win32 and Rust callbacks
            # remain unchanged. Snapshot each writable object section independently,
            # including private/static globals, rather than guessing vendor resets.
            nm=compiler.with_name('nm.exe'); objcopy=compiler.with_name('objcopy.exe')
            objdump=compiler.with_name('objdump.exe')
            symbols=set()
            for obj in built:
                for line in subprocess.check_output([str(nm),'-g','--defined-only',obj],env=env,text=True).splitlines():
                    fields=line.split()
                    if len(fields)==3: symbols.add(fields[2])
            mapping=objects/'symbols.map'
            refptr_sections = {f'.rdata${s}': f'.rdata$dvda_{group}_{s}'
                               for s in symbols if s.startswith('.refptr.')}
            mapping.write_text(''.join(f'{s} dvda_{group}_{s}\n' for s in sorted(symbols))
                               + ''.join(f'{s} {renamed}\n' for s, renamed in sorted(refptr_sections.items())),
                               encoding='ascii')
            reset=['#include <string.h>\n#include <stddef.h>\n']
            states=[]
            for index,obj in enumerate(built):
                if Path(obj).stem=='session-rust': continue
                sections=subprocess.check_output([str(objdump),'-h',obj],env=env,text=True).splitlines()
                additions=[]
                for symbol in symbols:
                    if symbol.startswith('.refptr.'):
                        additions.extend(['--rename-section',f'.rdata${symbol}=.rdata$dvda_{group}_{symbol}'])
                for line in sections:
                    fields=line.split()
                    if len(fields)<3 or not fields[0].isdigit(): continue
                    section=fields[1]; size=int(fields[2],16)
                    if not size or not (section.startswith('.data') or section.startswith('.bss')): continue
                    symbol=f'dvda_{group}_state_{index}_{len(states)}'
                    additions.extend(['--add-symbol',f'{symbol}={section}:0,global'])
                    reset.append(f'extern unsigned char {symbol}[{size}];\nstatic unsigned char saved_{symbol}[{size}];\n')
                    states.append(symbol)
                subprocess.run([str(objcopy),'--redefine-syms='+str(mapping),*additions,obj],env=env,check=True)
            session=next(obj for obj in built if Path(obj).stem=='session-rust')
            subprocess.run([str(objcopy),'--redefine-syms='+str(mapping),session],env=env,check=True)
            reset.append(f'void dvda_{group}_menu_vendor_reset(void) {{ static int captured;\n')
            for symbol in states:
                reset.append(f'if(!captured)memcpy(saved_{symbol},{symbol},sizeof({symbol})); else memcpy({symbol},saved_{symbol},sizeof({symbol}));\n')
            reset.append('captured=1; }\n')
            reset_source=objects/'reset.c'; reset_source.write_text(''.join(reset),encoding='ascii')
            reset_object=objects/'reset.o'
            subprocess.run([str(compiler),'-O2','-c',str(reset_source),'-o',str(reset_object)],env=env,check=True)
            # The reset declaration is undefined in session, so namespace it explicitly.
            subprocess.run([str(objcopy),'--redefine-sym',f'menu_vendor_reset=dvda_{group}_menu_vendor_reset',session],env=env,check=True)
            target=out/f'libdvda_menu_{group}_vendor.a'
            if target.exists(): target.unlink()
            subprocess.run([str(compiler.with_name('ar.exe')),'rcs',str(target),*built,str(reset_object)],env=env,check=True)
            files[target.name]={'bytes':target.stat().st_size,'sha256':sha(target),'state_sections':len(states)}
            continue
        dll=out/('dvda-menu-'+group+'.dll')
        cmd=[str(compiler),'-shared','-static-libgcc','-s','-Wl,--gc-sections','-Wl,--no-insert-timestamp',
             '-Wl,--exclude-all-symbols',*built,
             str(archive),'-lws2_32','-luserenv','-lbcrypt','-lntdll','-ladvapi32',
             '-lm','-o',str(dll)]
        with (out/(group+'-link.log')).open('wb') as log:
            subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
        imports=sorted(Pe(dll).imports())
        if any(not n.lower().startswith(('api-ms-win-', 'ext-ms-win-')) and not (Path(os.environ['SystemRoot'])/'System32'/n).exists() for n in imports):
            raise ValueError('Unexpected menu runtime import: '+str(imports))
        files[dll.name]={'bytes':dll.stat().st_size,'sha256':sha(dll),'imports':imports}
    record={'target':'Windows x64','scope':'DVD menu subpictures and DVD-Audio AMGM navigation',
            'adapter':'rust', 'rustc':rustc_version,
            'rust_inputs':rust_inputs, 'rust_archive_sha256':sha(archive) if not a.direct_link else None,
            'linkage':'static-vendor' if a.direct_link else 'differential-dll',
            'compiler':subprocess.check_output([str(compiler),'--version'],env=env).decode().splitlines()[0],
            'files':files,'source_inputs':{f.relative_to(root).as_posix():sha(f) for f in sorted(root.rglob('*')) if f.is_file()}}
    (out/'menu-build.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    (out/'NOTICE.txt').write_text('Menu algorithms derived from patched dvdauthor 0.7.1.\nLPCM byte packing derived from GPL-3.0-or-later dvda-author: tools/dvda-author-mlp8/src/audio.c.\nSource: tools/menu-native/vendor; provenance: ORIGIN.json.\n\n'+(vendor/'COPYING').read_text(encoding='utf-8'),encoding='utf-8')
    print(json.dumps(files),flush=True)

if __name__=='__main__':main()
