"""Verify Rust vendor resets against real COFF initialized/BSS/relocated state.

Two independent vendor groups contain private C data, zero storage and relocated
pointers. The Rust harness corrupts every byte of every writable section,
including padding, then checks exact restoration and isolation over 32 cycles.
Only the fixture is C; the production reset is generated and compiled by rustc.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import uuid
from coff_menu import Coff
from msvc_env import msvc_environment


def run(command, env, **arguments):
    return subprocess.run([str(value) for value in command], env=env, check=True, **arguments)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work-directory',type=Path)
    args=parser.parse_args()
    repo=Path(__file__).resolve().parents[2]
    work=(args.work_directory or repo/'build'/('menu-rust-reset-'+uuid.uuid4().hex[:8])).resolve()
    if work.exists() and any(work.iterdir()):
        raise ValueError('Reset acceptance needs a new or empty work directory: '+str(work))
    work.mkdir(parents=True,exist_ok=True)
    spec=importlib.util.spec_from_file_location('menu_builder',Path(__file__).with_name('build-menu-runtime.py'))
    builder=importlib.util.module_from_spec(spec);spec.loader.exec_module(builder)
    env=msvc_environment()
    compiler=Path(env['CC'])
    dumpbin=Path(shutil.which('dumpbin.exe',path=env['PATH']))
    rustc=Path.home()/'.cargo/bin/rustc.exe'
    groups={}; objects=[]
    for group,value in [('spu',41),('nav',83)]:
        folder=work/group;folder.mkdir()
        source=folder/'fixture.c';obj=folder/'fixture.obj'
        source.write_text(f'''
/* Test-only private vendor state, including deliberately separate BSS. */
static unsigned char initialized[19] = {{9, 8, 7, 6}};
#pragma bss_seg(".bss$z")
static unsigned char zero[35];
#pragma bss_seg()
static int target={value}, alternate={value+1};
static int *relocated=&target;
int fixture_{group}_initial(void) {{
    for(int i=0;i<19;i++)if(initialized[i]!=(i<4 ? 9-i : 0))return 0;
    for(int i=0;i<35;i++)if(zero[i])return 0;
    return target=={value} && alternate=={value+1} && relocated==&target;
}}
void fixture_{group}_touch(void) {{
    initialized[0]++;zero[0]++;target++;alternate++;relocated=&alternate;
}}
''',encoding='ascii')
        run([compiler,'/nologo','/O2','/std:c11','/MT','/Gy-','/Gw-','/c',source,'/Fo'+str(obj)],env)
        states=Coff(obj).rewrite({},f'dvda_{group}_state_0')
        if not any(state['section'].startswith('.bss') for state in states):
            raise AssertionError('Fixture must include an actual BSS section')
        reset,metadata=builder.compile_rust_reset(group,states,folder,rustc,dumpbin,env)
        groups[group]=metadata
        objects.extend([obj,reset])
    harness=['unsafe extern "C" {\n']
    for group,metadata in groups.items():
        harness.append(f'    fn dvda_{group}_menu_vendor_reset();\n    fn fixture_{group}_initial()->i32;\n')
        for state in metadata['sections']:
            harness.append(f'    static mut {state["symbol"]}: [u8; {state["bytes"]}];\n')
    harness.append('}\n')
    for group,metadata in groups.items():
        harness.append(f'fn regions_{group}()->Vec<(*mut u8,usize)> {{ vec![\n')
        for state in metadata['sections']:
            harness.append(f'    ((&raw mut {state["symbol"]}).cast::<u8>(),{state["bytes"]}),\n')
        harness.append('] }\n')
    harness.append('''
unsafe fn snapshot(regions:&[(*mut u8,usize)])->Vec<Vec<u8>> {
    regions.iter().map(|&(p,n)|unsafe{std::slice::from_raw_parts(p,n).to_vec()}).collect()
}
unsafe fn fill(regions:&[(*mut u8,usize)],value:u8) {
    for &(p,n) in regions { unsafe{p.write_bytes(value,n);} }
}
fn main() {
    let spu=regions_spu();let nav=regions_nav();
    unsafe {
        assert_eq!(fixture_spu_initial(),1);assert_eq!(fixture_nav_initial(),1);
        let expected_spu=snapshot(&spu);let expected_nav=snapshot(&nav);
        dvda_spu_menu_vendor_reset();dvda_nav_menu_vendor_reset();
        for cycle in 1..=32 {
            fill(&spu,cycle);fill(&nav,cycle+64);
            let changed_nav=snapshot(&nav);
            dvda_spu_menu_vendor_reset();
            assert_eq!(snapshot(&spu),expected_spu);assert_eq!(snapshot(&nav),changed_nav);
            assert_eq!(fixture_spu_initial(),1);
            fill(&spu,cycle+128);let changed_spu=snapshot(&spu);
            dvda_nav_menu_vendor_reset();
            assert_eq!(snapshot(&nav),expected_nav);assert_eq!(snapshot(&spu),changed_spu);
            assert_eq!(fixture_nav_initial(),1);
            dvda_spu_menu_vendor_reset();assert_eq!(snapshot(&spu),expected_spu);
        }
    }
    println!("Rust reset private initialized/BSS/pointer/padding and interleaved group isolation PASS (32 cycles)");
}
''')
    harness_source=work/'harness.rs';harness_source.write_text(''.join(harness),encoding='ascii')
    executable=work/'reset-acceptance.exe'
    command=[rustc,'--edition=2024','--target','x86_64-pc-windows-msvc','-C','panic=abort',
             '-C','target-feature=+crt-static','-O',harness_source,'-o',executable]
    for obj in objects: command.extend(['-C','link-arg='+str(obj)])
    run(command,env)
    result=run([executable],env,capture_output=True,text=True)
    print(result.stdout.strip(),flush=True)
    report={'schema_version':1,'passed':True,'cycles':32,'groups':groups,
            'checks':['initialized-data','zero-bss','relocated-pointers','private-static-data','section-padding','group-isolation'],
            'executable_sha256':builder.sha(executable),'harness_sha256':builder.sha(harness_source),
            'fixture_sha256':{group:builder.sha(work/group/'fixture.c') for group in groups}}
    (work/'reset-acceptance.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print('Rust reset acceptance: '+str(work/'reset-acceptance.json'),flush=True)


if __name__=='__main__':main()
