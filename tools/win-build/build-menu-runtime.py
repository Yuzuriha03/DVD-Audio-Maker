"""Build the vendor menu algorithms with the Rust XML/resource boundary."""
from pathlib import Path
import argparse, concurrent.futures, hashlib, json, os, subprocess, sys
sys.path.insert(0, str(Path(__file__).parent))
from worker_policy import worker_count
sys.path.insert(0,str(Path(__file__).parent/'native'))
from pe_dependencies import Pe

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def source_inputs(root):
    return {f.relative_to(root).as_posix():sha(f) for f in sorted(root.rglob('*')) if f.is_file()}

def rust_inputs(repo, crate):
    inputs=[repo/'rust/Cargo.toml',repo/'rust/Cargo.lock',Path(__file__).resolve(),*sorted(crate.rglob('*.rs')),crate/'Cargo.toml']
    return {f.relative_to(repo).as_posix():sha(f) for f in inputs}

def writable_sections(section_output):
    """Preserve the vendor snapshot's complete .data/.bss section coverage."""
    result=[]
    for line in section_output.splitlines():
        fields=line.split()
        if len(fields)<3 or not fields[0].isdigit(): continue
        section=fields[1]; size=int(fields[2],16)
        if size and section.startswith(('.data','.bss')):
            result.append((section,size))
    return result

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

def compile_rust_reset(group, states, objects, rustc, nm, env):
    source=objects/'reset.rs'; output=objects/'reset.o'
    source.write_text(rust_reset_source(group,states),encoding='ascii')
    command=[str(rustc),'--crate-name',f'dvda_menu_{group}_reset','--crate-type=lib',
             '--emit=obj','--target','x86_64-pc-windows-gnu','--edition=2024',
             '-C','panic=abort','-O',str(source),'-o',str(output)]
    result=subprocess.run(command,env=env,capture_output=True)
    log=objects/'reset-rust.log';log.write_bytes(result.stdout+result.stderr)
    if result.returncode: raise RuntimeError('Rust reset compilation failed: '+str(log))
    definitions=subprocess.check_output([str(nm),'-g','--defined-only',str(output)],env=env,text=True)
    expected=f'dvda_{group}_menu_vendor_reset'
    if expected not in {line.split()[-1] for line in definitions.splitlines() if line.split()}:
        raise ValueError('Rust reset is missing its vendor ABI export: '+expected)
    undefined=subprocess.check_output([str(nm),'--undefined-only',str(output)],env=env,text=True)
    imports={line.split()[-1] for line in undefined.splitlines() if line.split()}
    allowed={state['symbol'] for state in states}|{'memcpy','memmove','memset'}
    if imports-allowed:
        raise ValueError('Standalone Rust reset has unexpected std/panic/import dependencies: '+str(sorted(imports-allowed)))
    metadata={'implementation':'rust','target':'x86_64-pc-windows-gnu','no_std':True,'panic_strategy':'abort',
              'generated_source':source.as_posix(),'generated_source_sha256':sha(source),
              'object':output.as_posix(),'object_sha256':sha(output),'command':command,
              'undefined_symbols':sorted(imports),'sections':states}
    return output,metadata

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--msys-root',type=Path,default=Path('C:/msys64'))
    p.add_argument('--direct-link', action='store_true', help='Build namespaced vendor archives only (no owned DLL or Cargo recursion)')
    p.add_argument('--output',type=Path,default=Path('build/menu-native'))
    a=p.parse_args();root=Path(__file__).resolve().parents[1]/'menu-native';vendor=root/'vendor'
    repo=root.parents[1]
    cargo=Path.home()/'.cargo/bin/cargo.exe'
    rustc=cargo.with_name('rustc.exe')
    rustc_version=subprocess.check_output([str(rustc),'--version']).decode().strip()
    crate=repo/'rust/crates/dvda-menu'
    session_source=crate/'src/session.rs'
    if not session_source.is_file() or 'menu_rust_run(' not in (root/'session-rust.c').read_text(encoding='utf-8'):
        raise ValueError('Menu producer requires the Rust session coordinator and its C boundary call')
    rust_sources=rust_inputs(repo,crate); vendor_sources=source_inputs(root)
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
    files={}; state_resets={}
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
            states=[]
            for index,obj in enumerate(built):
                sections=subprocess.check_output([str(objdump),'-h',obj],env=env,text=True)
                additions=[]
                for symbol in symbols:
                    if symbol.startswith('.refptr.'):
                        additions.extend(['--rename-section',f'.rdata${symbol}=.rdata$dvda_{group}_{symbol}'])
                # Session state belongs to the Rust coordinator, but its COFF
                # refptr COMDAT sections still need the same group namespace.
                if Path(obj).stem!='session-rust':
                    for section,size in writable_sections(sections):
                        symbol=f'dvda_{group}_state_{index}_{len(states)}'
                        additions.extend(['--add-symbol',f'{symbol}={section}:0,global'])
                        states.append({'symbol':symbol,'bytes':size,'object':Path(obj).name,'section':section})
                subprocess.run([str(objcopy),'--redefine-syms='+str(mapping),*additions,obj],env=env,check=True)
            session=next(obj for obj in built if Path(obj).stem=='session-rust')
            reset_object,reset_record=compile_rust_reset(group,states,objects,rustc,nm,env)
            for field in ('generated_source','object'):
                reset_record[field]=str(Path(reset_record[field]).relative_to(out).as_posix())
            state_resets[group]=reset_record
            # The reset declaration is undefined in session, so namespace it explicitly.
            subprocess.run([str(objcopy),'--redefine-sym',f'menu_vendor_reset=dvda_{group}_menu_vendor_reset',session],env=env,check=True)
            target=out/f'libdvda_menu_{group}_vendor.a'
            if target.exists(): target.unlink()
            subprocess.run([str(compiler.with_name('ar.exe')),'rcs',str(target),*built,str(reset_object)],env=env,check=True)
            files[target.name]={'bytes':target.stat().st_size,'sha256':sha(target),'state_sections':len(states),'state_reset':reset_record}
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
    if rust_inputs(repo,crate)!=rust_sources or source_inputs(root)!=vendor_sources:
        raise RuntimeError('Menu sources changed during the build; rerun to authenticate stable binaries')
    record={'target':'Windows x64','scope':'DVD menu subpictures and DVD-Audio AMGM navigation',
            'adapter':'rust', 'rustc':rustc_version,
            'rust_inputs':rust_sources, 'rust_archive_sha256':sha(archive) if not a.direct_link else None,
            'session':{'implementation':'rust','source':'rust/crates/dvda-menu/src/session.rs','boundary':'c-setjmp-varargs'},
            'state_reset':{'implementation':'rust','groups':state_resets} if a.direct_link else
                          {'implementation':'not-required','strategy':'fresh-dll-state'},
            'linkage':'static-vendor' if a.direct_link else 'differential-dll',
            'compiler':subprocess.check_output([str(compiler),'--version'],env=env).decode().splitlines()[0],
            'files':files,'source_inputs':vendor_sources}
    (out/'menu-build.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    (out/'NOTICE.txt').write_text('Menu algorithms derived from patched dvdauthor 0.7.1.\nLPCM byte packing translated from GPL-3.0-or-later dvda-author audio.c.\nHistorical author source revision/hashes: rust/crates/dvda-author/tests/fixtures/legacy-source-provenance.json.\nMenu source: tools/menu-native/vendor; provenance: ORIGIN.json.\n\n'+(vendor/'COPYING').read_text(encoding='utf-8'),encoding='utf-8')
    print(json.dumps({name:{key:value for key,value in metadata.items() if key!='state_reset'}
                      for name,metadata in files.items()}),flush=True)

if __name__=='__main__':main()
