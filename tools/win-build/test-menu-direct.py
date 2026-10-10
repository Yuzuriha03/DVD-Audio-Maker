"""Targeted static menu executable parity and repeat/error recovery (no menu DLL)."""
from pathlib import Path
import argparse, json, os, subprocess

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--fixtures',type=Path,default=Path('build/menu-production-pal-1'))
    p.add_argument('--image-runtime',type=Path,default=Path('build/rust-image-runtime/dvda-image.dll'))
    p.add_argument('--bridge-archive',type=Path,help='Test author ABI with directly linked image/menu bridge archive')
    p.add_argument('--output',type=Path,default=Path('build/menu-direct-acceptance'))
    a=p.parse_args(); repo=Path(__file__).resolve().parents[2]; out=a.output.resolve(); out.mkdir(parents=True,exist_ok=True)
    fixture=a.fixtures.resolve(); image=a.image_runtime.resolve()
    source=out/'menu-direct-test.c'
    source.write_text(r'''
#include <windows.h>
#include <shellapi.h>
#include <stdlib.h>
#include <stdio.h>
#include "menu-api.h"
extern int dvda_menu_run_spu(const DvdaMenuRequest *);
extern int dvda_menu_run_navigation(const DvdaMenuRequest *);
#ifdef AUTHOR_BRIDGE
extern int dvda_menu_subpictures(const char *,const char *,const char *);
extern int dvda_menu_navigation(const char *,const char *);
static int bridge_spu(const DvdaMenuRequest *r){return dvda_menu_subpictures(r->xml,r->input,r->output);}
static int bridge_nav(const DvdaMenuRequest *r){return dvda_menu_navigation(r->xml,r->output);}
#define dvda_menu_run_spu bridge_spu
#define dvda_menu_run_navigation bridge_nav
#endif
int wmain(int argc,wchar_t **wide_args) {
    wide_args=CommandLineToArgvW(GetCommandLineW(),&argc);
    if(argc!=9)return 2;
    char *argv[9];
    for(int i=0;i<argc;i++) {
        int n=WideCharToMultiByte(CP_UTF8,0,wide_args[i],-1,NULL,0,NULL,NULL);
        argv[i]=malloc(n);WideCharToMultiByte(CP_UTF8,0,wide_args[i],-1,argv[i],n,NULL,NULL);
    }
    DvdaMenuRequest r={sizeof(r),argv[2],argv[3],argv[4],NULL};
#ifdef AUTHOR_BRIDGE
    if(!dvda_menu_subpictures(NULL,argv[3],argv[4]))return 12;
    if(!dvda_menu_subpictures(argv[2],NULL,argv[4]))return 13;
    if(!dvda_menu_subpictures(argv[2],argv[3],NULL))return 14;
    if(!dvda_menu_navigation(NULL,argv[8]))return 15;
    if(!dvda_menu_navigation(argv[5],NULL))return 16;
#endif
#ifndef AUTHOR_BRIDGE
    HMODULE image=LoadLibraryW(wide_args[1]);if(!image)return 3;
    r.read_rgba=(DvdaReadRgba)GetProcAddress(image,"dvda_image_read_rgba");
    if(!r.read_rgba)return 4;
#endif
    for(int i=0;i<4;i++) {
        r.xml="missing-menu-direct.xml";if(!dvda_menu_run_spu(&r))return 5;
        r.xml=argv[2];if(dvda_menu_run_spu(&r))return 6;
        r.xml=argv[6];if(!dvda_menu_run_spu(&r))return 9;
        r.xml=argv[2];if(dvda_menu_run_spu(&r))return 6;
        r.xml=argv[2];r.input=argv[3];r.output=argv[4];
    }
    r.xml=argv[5];r.input=NULL;r.output=argv[8];
    for(int i=0;i<4;i++) {
        r.xml="missing-nav.xml";if(!dvda_menu_run_navigation(&r))return 7;
        r.xml=argv[5];if(dvda_menu_run_navigation(&r))return 8;
    }
    r.xml=argv[7];if(!dvda_menu_run_navigation(&r))return 10;
    r.xml=argv[5];if(dvda_menu_run_navigation(&r))return 11;
    for(int i=0;i<argc;i++)free(argv[i]);LocalFree(wide_args);
#ifndef AUTHOR_BRIDGE
    FreeLibrary(image);
#endif
    return 0;
}
''',encoding='ascii')
    gcc=Path('C:/msys64/mingw64/bin/gcc.exe'); env=os.environ|{'PATH':str(gcc.parent)+os.pathsep+os.environ['PATH']}
    exe=out/'menu-direct-test.exe'; archive=repo/'rust/target/x86_64-pc-windows-gnu/release/libdvda_menu.a'
    if a.bridge_archive:
        archive=a.bridge_archive.resolve()
    bridge_flags=['-DAUTHOR_BRIDGE','-lgdi32'] if a.bridge_archive else []
    subprocess.run([str(gcc),str(source),'-I'+str(repo/'tools/menu-native'),str(archive),'-municode','-static-libgcc','-lws2_32','-luserenv','-lbcrypt','-lntdll','-ladvapi32','-lshell32',*bridge_flags,'-o',str(exe)],env=env,check=True)
    target=out/'menu.mpg'; nav=out/'nav'; (nav/'AUDIO_TS').mkdir(parents=True,exist_ok=True);(nav/'VIDEO_TS').mkdir(exist_ok=True)
    bad_spu=out/'bad-spu.xml'; bad_spu.write_text((fixture/'\u83dc\u5355.xml').read_text(encoding='utf-8').replace('force="yes"','force="invalid"'),encoding='utf-8')
    bad_boolean=out/'bad-boolean-nav.xml'; bad_boolean.write_text('<dvdauthor jumppad="invalid"/>',encoding='utf-8')
    subprocess.run([str(exe),str(image),str(fixture/'\u83dc\u5355.xml'),str(fixture/'\u80cc\u666f.mpg'),str(target),str(fixture/'\u5bfc\u822a.xml'),str(bad_spu),str(bad_boolean),str(nav)],env=env,check=True)
    if target.read_bytes()!=(fixture/'legacy.mpg').read_bytes():raise AssertionError('SPU parity')
    expected={str(f.relative_to(fixture/'legacy-nav')):f.read_bytes() for f in (fixture/'legacy-nav').rglob('*') if f.is_file()}
    actual={str(f.relative_to(nav)):f.read_bytes() for f in nav.rglob('*') if f.is_file()}
    if not expected or expected!=actual:raise AssertionError('navigation parity')
    report={'status':'PASS','checks':['static executable SPU byte parity','same-process SPU recovery x8','same-process navigation recovery x4','navigation byte parity','vendor C error containment'],'author_abi':bool(a.bridge_archive),'executable':str(exe),'fixtures':str(fixture)}
    (out/'report.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8'); print(json.dumps(report))
if __name__=='__main__':main()
