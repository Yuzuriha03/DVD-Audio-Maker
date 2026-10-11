"""Build the vendor menu algorithms with the Rust XML/resource boundary."""
from pathlib import Path
import argparse, concurrent.futures, hashlib, json, os, shutil, subprocess, sys
sys.path.insert(0, str(Path(__file__).parent))
from worker_policy import worker_count
from msvc_env import msvc_environment
from coff_menu import Coff
sys.path.insert(0,str(Path(__file__).parent/'native'))
from pe_dependencies import Pe

TARGET = 'x86_64-pc-windows-msvc'

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def source_inputs(root):
    return {f.relative_to(root).as_posix():sha(f) for f in sorted(root.rglob('*')) if f.is_file()}

def rust_inputs(repo, crate):
    inputs=[repo/'rust/Cargo.toml',repo/'rust/Cargo.lock',Path(__file__).resolve(),
            Path(__file__).with_name('msvc_env.py'),Path(__file__).with_name('coff_menu.py'),
            *sorted(crate.rglob('*.rs')),crate/'Cargo.toml']
    return {f.relative_to(repo).as_posix():sha(f) for f in inputs}

def rust_reset_source(group, states):
    """Generate a standalone no_std reset, avoiding Cargo/build-script recursion.

    The exported byte arrays alias whole vendor sections, including private
    globals and padding. First entry captures relocated process initial state;
    subsequent entries restore it. The direct-session mutex owns synchronization.
    """
    code=['#![no_std]\n',
          '// Generated writable vendor state snapshot; called under the direct-session mutex.\n',
          'static mut CAPTURED: bool = false;\n', 'unsafe extern "C" {\n']
    for state in states:
        code.append(f'    static mut {state["symbol"]}: [u8; {state["bytes"]}];\n')
    code.append('}\n')
    for rank,state in enumerate(states):
        code.append(f'static mut SAVED_{rank}: [u8; {state["bytes"]}] = [0; {state["bytes"]}];\n')
    code.extend(['\n/// # Safety\n',
                 '/// Hold the direct-session mutex and finish vendor resource cleanup before restoring.\n',
                 '#[unsafe(no_mangle)]\n',
                 f'pub unsafe extern "C" fn dvda_{group}_menu_vendor_reset() {{\n',
                 '    unsafe {\n', '        if (&raw const CAPTURED).read() {\n'])
    for rank,state in enumerate(states):
        code.append(f'            core::ptr::copy_nonoverlapping((&raw const SAVED_{rank}).cast::<u8>(), '
                    f'(&raw mut {state["symbol"]}).cast::<u8>(), {state["bytes"]});\n')
    code.append('        } else {\n')
    for rank,state in enumerate(states):
        code.append(f'            core::ptr::copy_nonoverlapping((&raw const {state["symbol"]}).cast::<u8>(), '
                    f'(&raw mut SAVED_{rank}).cast::<u8>(), {state["bytes"]});\n')
    code.extend(['            (&raw mut CAPTURED).write(true);\n', '        }\n', '    }\n', '}\n'])
    return ''.join(code)

def compile_rust_reset(group, states, objects, rustc, dumpbin, env):
    source=objects/'reset.rs'; output=objects/'reset.obj'
    source.write_text(rust_reset_source(group,states),encoding='ascii')
    command=[str(rustc),'--crate-name',f'dvda_menu_{group}_reset','--crate-type=lib',
             '--emit=obj','--target',TARGET,'--edition=2024',
             '-C','panic=abort','-C','target-feature=+crt-static','-O',str(source),'-o',str(output)]
    result=subprocess.run(command,env=env,capture_output=True)
    log=objects/'reset-rust.log';log.write_bytes(result.stdout+result.stderr)
    if result.returncode: raise RuntimeError('Rust reset compilation failed: '+str(log))
    audit=subprocess.check_output([str(dumpbin),'/nologo','/symbols',str(output)],env=env)
    (objects/'reset-symbols.log').write_bytes(audit)
    symbols=Coff(output)
    expected=f'dvda_{group}_menu_vendor_reset'
    if expected not in symbols.definitions():
        raise ValueError('Rust reset is missing its vendor ABI export: '+expected)
    imports=symbols.undefined()
    allowed={state['symbol'] for state in states}|{'memcpy','memmove','memset'}
    if imports-allowed:
        raise ValueError('Standalone Rust reset has unexpected std/panic/import dependencies: '+str(sorted(imports-allowed)))
    metadata={'implementation':'rust','target':TARGET,'toolchain':'msvc','crt':'static','no_std':True,'panic_strategy':'abort',
              'generated_source':source.as_posix(),'generated_source_sha256':sha(source),
              'object':output.as_posix(),'object_sha256':sha(output),'command':command,
              'undefined_symbols':sorted(imports),'sections':states}
    return output,metadata

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--target',choices=(TARGET,),default=TARGET,
                   help='MSVC target (the only supported target)')
    p.add_argument('--direct-link', action='store_true', help='Build namespaced vendor archives only (no owned DLL or Cargo recursion)')
    p.add_argument('--output',type=Path,default=Path('build/menu-native'))
    a=p.parse_args()
    if a.target != TARGET:
        p.error(f'MSVC-only menu build requires --target {TARGET}')
    root=Path(__file__).resolve().parents[1]/'menu-native';vendor=root/'vendor'
    repo=root.parents[1]
    cargo=Path.home()/'.cargo/bin/cargo.exe'
    rustc=cargo.with_name('rustc.exe')
    rustc_version=subprocess.check_output([str(rustc),'--version']).decode().strip()
    crate=repo/'rust/crates/dvda-menu'
    session_source=crate/'src/session.rs'
    if not session_source.is_file() or 'menu_rust_run(' not in (root/'session-rust.c').read_text(encoding='utf-8'):
        raise ValueError('Menu producer requires the Rust session coordinator and its C boundary call')
    rust_sources=rust_inputs(repo,crate); vendor_sources=source_inputs(root)
    env=msvc_environment()
    env['RUSTFLAGS']=env.get('RUSTFLAGS','')+' -C target-feature=+crt-static'
    if not a.direct_link:
        subprocess.run([str(cargo),'build','--locked','--manifest-path',str(repo/'rust/Cargo.toml'),'-p','dvda-menu',
                        '--release','--target',TARGET],env=env,check=True)
    archive=repo/'rust/target'/TARGET/'release/dvda_menu.lib'
    out=a.output.resolve();out.mkdir(parents=True,exist_ok=True)
    compiler=Path(env['CC']); librarian=Path(env['DVDA_LIB_TOOL'])
    dumpbin=Path(shutil.which('dumpbin.exe',path=env['PATH']))
    linker=Path(shutil.which('link.exe',path=env['PATH']))
    flags=['/nologo','/O2','/std:c11','/MT','/utf-8','/Zl','/Gy-','/Gw-',
           '/D_CRT_SECURE_NO_WARNINGS','/D_CRT_NONSTDC_NO_WARNINGS','/D_USE_MATH_DEFINES',
           '/I'+str(root/'msvc'),'/I'+str(root),'/I'+str(vendor),'/FI'+str(root/'session.h')]
    groups={'spu':['subgen','subgen-parse-xml','subgen-encode','subgen-image','compat'],
            'nav':['dvdauthor','dvdcompile','dvdvml','dvdvmy','dvdifo','dvdvob','dvdpgc','dvdcli','compat']}
    files={}; state_resets={}
    for group,names in groups.items():
        objects=out/('objects-'+group);objects.mkdir(exist_ok=True)
        sources=[vendor/(n+'.c') for n in names]+[root/'session-rust.c']
        group_flags=flags+(['/DDVDA_SUBPICTURE'] if group=='spu' else [])+(['/DDVDA_DIRECT_LINK'] if a.direct_link else [])
        def compile_one(source):
            target=objects/(source.stem+'.obj')
            local_flags=group_flags+(['/DDVDA_SESSION_IMPLEMENTATION'] if source.name == 'session-rust.c' else [])
            r=subprocess.run([str(compiler),*local_flags,'/c',str(source),'/Fo'+str(target)],env=env,capture_output=True)
            log=objects/(source.stem+'.log');log.write_bytes(r.stdout+r.stderr)
            if r.returncode:raise RuntimeError('Compilation failed: '+str(log))
            return str(target)
        with concurrent.futures.ThreadPoolExecutor(max_workers=worker_count(len(sources))) as pool:
            built=list(pool.map(compile_one,sources))
        if a.direct_link:
            symbols=set().union(*(Coff(obj).definitions() for obj in built))
            renames={symbol:f'dvda_{group}_{symbol}' for symbol in symbols}
            renames['menu_vendor_reset']=f'dvda_{group}_menu_vendor_reset'
            mapping=objects/'symbols.map'
            mapping.write_text(''.join(f'{s} {renamed}\n' for s,renamed in sorted(renames.items())),
                               encoding='ascii')
            states=[]
            for index,obj in enumerate(built):
                prefix=f'dvda_{group}_state_{index}' if Path(obj).stem!='session-rust' else None
                states.extend(Coff(obj).rewrite(renames,prefix))
                audit=subprocess.check_output([str(dumpbin),'/nologo','/symbols',obj],env=env)
                (objects/(Path(obj).stem+'-symbols.log')).write_bytes(audit)
            reset_object,reset_record=compile_rust_reset(group,states,objects,rustc,dumpbin,env)
            for field in ('generated_source','object'):
                reset_record[field]=str(Path(reset_record[field]).relative_to(out).as_posix())
            state_resets[group]=reset_record
            target=out/f'dvda_menu_{group}_vendor.lib'
            if target.exists(): target.unlink()
            subprocess.run([str(librarian),'/nologo','/OUT:'+str(target),*built,str(reset_object)],env=env,check=True)
            files[target.name]={'bytes':target.stat().st_size,'sha256':sha(target),'state_sections':len(states),'state_reset':reset_record}
            continue
        dll=out/('dvda-menu-'+group+'.dll')
        cmd=[str(linker),'/NOLOGO','/DLL','/OUT:'+str(dll),'/MACHINE:X64','/OPT:REF',*built,
             str(archive),'libcmt.lib','libvcruntime.lib','libucrt.lib','legacy_stdio_definitions.lib',
             'ws2_32.lib','userenv.lib','bcrypt.lib','ntdll.lib','advapi32.lib','kernel32.lib',
             'ole32.lib','windowscodecs.lib']
        with (out/(group+'-link.log')).open('wb') as log:
            subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
        imports=sorted(Pe(dll).imports())
        if any(not n.lower().startswith(('api-ms-win-', 'ext-ms-win-')) and not (Path(os.environ['SystemRoot'])/'System32'/n).exists() for n in imports):
            raise ValueError('Unexpected menu runtime import: '+str(imports))
        files[dll.name]={'bytes':dll.stat().st_size,'sha256':sha(dll),'imports':imports}
    if rust_inputs(repo,crate)!=rust_sources or source_inputs(root)!=vendor_sources:
        raise RuntimeError('Menu sources changed during the build; rerun to authenticate stable binaries')
    record={'target':TARGET,'toolchain':'msvc','crt':'static','scope':'DVD menu subpictures and DVD-Audio AMGM navigation',
            'adapter':'rust', 'rustc':rustc_version,
            'rust_inputs':rust_sources, 'rust_archive_sha256':sha(archive) if not a.direct_link else None,
            'session':{'implementation':'rust','source':'rust/crates/dvda-menu/src/session.rs','boundary':'c-setjmp-varargs'},
            'state_reset':{'implementation':'rust','groups':state_resets} if a.direct_link else
                          {'implementation':'not-required','strategy':'fresh-dll-state'},
            'linkage':'static-vendor' if a.direct_link else 'differential-dll',
            'compiler':subprocess.run([str(compiler)],env=env,capture_output=True).stderr.decode(errors='replace').splitlines()[0],
            'files':files,'source_inputs':vendor_sources}
    (out/'menu-build.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    (out/'NOTICE.txt').write_text('Menu algorithms derived from patched dvdauthor 0.7.1.\nLPCM byte packing translated from GPL-3.0-or-later dvda-author audio.c.\nHistorical author source revision/hashes: rust/crates/dvda-author/tests/fixtures/legacy-source-provenance.json.\nMenu source: tools/menu-native/vendor; provenance: ORIGIN.json.\n\n'+(vendor/'COPYING').read_text(encoding='utf-8'),encoding='utf-8')
    print(json.dumps({name:{key:value for key,value in metadata.items() if key!='state_reset'}
                      for name,metadata in files.items()}),flush=True)

if __name__=='__main__':main()
