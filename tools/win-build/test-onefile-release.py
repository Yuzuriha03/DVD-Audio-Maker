"""Test a standalone EXE, verified runtime cache, and real GUI authoring."""
from pathlib import Path
import argparse, concurrent.futures, hashlib, json, os, shutil, subprocess, sys, time, zipfile

def sha(path):
    with Path(path).open('rb') as file:return hashlib.file_digest(file,'sha256').hexdigest()

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--exe',type=Path,required=True)
    p.add_argument('--baseline',type=Path,required=True)
    p.add_argument('--previous',type=Path,required=True)
    p.add_argument('--fixtures',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--startup-only',action='store_true')
    p.add_argument('--version',default='v1.0')
    p.add_argument('--package',type=Path,help='Release directory containing sidecars; defaults to --exe parent')
    p.add_argument('--provenance',type=Path,help='Local validated stage containing build records; never shipped')
    a=p.parse_args();work=a.output.resolve();work.mkdir(parents=True,exist_ok=False)
    package=(a.package or a.exe.parent).resolve()
    provenance=(a.provenance or Path(__file__).resolve().parents[2]/'tools/win-build/publish/package-stage-win-x64').resolve()
    release=work/'独立 EXE';release.mkdir();executable=release/'DVD-Audio-Maker.exe'
    shutil.copy2(a.exe,executable)
    cache=work/'组件 缓存';config=work/'project.env'
    config.write_text('DVDA_TITLE="独立发布 中文 日本語"\nDVDA_AUTHOR=""\nDVDA_AUTHOR_SRC=""\n',encoding='utf-8')
    clean={k:v for k,v in os.environ.items() if not k.upper().startswith(('DVDA_','MAGICK_','DOTNET_','COREHOST_','FONTCONFIG_'))}
    clean['PATH']=os.pathsep.join([os.environ['SystemRoot']+'/System32',os.environ['SystemRoot'],str(Path(shutil.which('dotnet')).parent)])
    env=clean|{'DVDA_BUNDLE_CACHE_ROOT':str(cache),'DVDA_GUI_SMOKE_DIRECTORY':str(work)}
    report={'status':'RUNNING','checks':[],'times':{},'exe':str(executable)}
    def check(name,condition):
        if not condition:raise AssertionError(name)
        report['checks'].append(name);print('PASS '+name,flush=True)
    def smoke(name,selected=config,language="en"):
        started=time.monotonic()
        result=subprocess.run([executable,'--smoke-test','--config',selected,'--language',language],cwd=work,
            env=env|{'DVDA_GUI_SMOKE_SETTINGS':str(work/(name+'.json'))},capture_output=True,timeout=120,creationflags=subprocess.CREATE_NO_WINDOW)
        report['times'][name]=round(time.monotonic()-started,3)
        (work/(name+'.stderr')).write_bytes(result.stderr)
        if result.returncode:raise AssertionError(name+': '+result.stderr.decode('utf-8','replace'))
        return json.loads((work/(name+'.json')).read_text('utf-8'))
    try:
        sidecars={'README.md','README.en.md','README.ja.md','RUNTIME.md','RUNTIME.en.md','RUNTIME.ja.md','LICENSE',
            'THIRD-PARTY.md','THIRD-PARTY.en.md','config.env.example',
            'NOTICE-Image.txt','NOTICE-Menu.txt'}
        check('User documentation and licenses accompany the EXE',all((package/name).is_file() for name in sidecars))
        check('Developer build JSON files are absent from the release',not list(package.rglob('*.json')))
        check('Example configuration cannot override user defaults',not (package/'config.env').exists())
        package_entries={line.split('  ',1)[1]:line.split('  ',1)[0] for line in
            (package/'MANIFEST.txt').read_text('utf-8').splitlines() if line and not line.startswith('#')}
        check('Release manifest contains exactly the EXE and sidecars',set(package_entries)==sidecars|{'DVD-Audio-Maker.exe'})
        check('All release files match the manifest',all(sha(package/name)==value for name,value in package_entries.items()))
        archive=package/f'DVD-Audio-Maker-{a.version}-win-x64.zip'
        with zipfile.ZipFile(archive) as zipped:
            check('Release ZIP contains only the intended files',set(zipped.namelist())==set(package_entries)|{'MANIFEST.txt'})
            check('All release files are at the ZIP root',all('/' not in name and chr(92) not in name for name in zipped.namelist()))
            check('Release ZIP bytes match the tested package',all(zipped.read(name)==(package/name).read_bytes() for name in zipped.namelist()))
        settings=smoke('cold')
        check('English GUI contains only current actions and encoding choices',bool(settings['Values']))
        check('Chinese GUI starts with current actions',bool(smoke('chinese',language='zh-CN')['Values']))
        japanese=smoke('japanese',language='ja')
        check('Japanese GUI starts and retains the selected language',japanese['Language']=='ja' and japanese['Values']==settings['Values'])
        lpcm=work/'lpcm.env';lpcm.write_text('DVDA_MLP_SOURCE=lpcm\nDVDA_MLP_SURCODE_BITS=24\nDVDA_TITLE=日本語テスト\n',encoding='utf-8')
        lpcm_settings=smoke('japanese-lpcm',lpcm,language='ja')
        check('Japanese LPCM choice preserves its configuration value',lpcm_settings['Values']['DVDA_MLP_SOURCE']=='lpcm')
        saved=work/'japanese-profile.json';saved.write_text(json.dumps(lpcm_settings),encoding='utf-8')
        env['DVDA_GUI_SMOKE_SWITCH_LANGUAGE']='ja'
        switched=smoke('switch-to-japanese',saved,language='en')
        del env['DVDA_GUI_SMOKE_SWITCH_LANGUAGE']
        check('Switching to Japanese preserves all project settings',switched==lpcm_settings)
        legacy=work/'legacy-import.env'
        legacy.write_text('DVDA_MLP_SOURCE=SURCODE\nDVDA_TITLE=Legacy import\n',encoding='utf-8')
        migrated=smoke('legacy-env',legacy)
        check('Legacy env imports as generic MLP without rewriting input',migrated['Values']['DVDA_MLP_SOURCE']=='external' and 'SURCODE' in legacy.read_text('utf-8'))
        legacy_json=work/'legacy-profile.json'
        migrated['Values']['DVDA_MLP_SOURCE']='surcode'
        legacy_json.write_text(json.dumps(migrated),encoding='utf-8')
        check('Legacy JSON imports as generic MLP',smoke('legacy-json',legacy_json)['Values']['DVDA_MLP_SOURCE']=='external')
        removed=subprocess.run([executable,'--smoke-test','--config',config,'--language','en'],cwd=work,
            env=env|{'DVDA_GUI_SMOKE_ACTION':'Preview'},capture_output=True,timeout=120,creationflags=subprocess.CREATE_NO_WINDOW)
        check('Removed GUI Preview action fails instead of executing a dry run',removed.returncode!=0)

        roots=list(cache.iterdir());check('One version cache created',len(roots)==1)
        runtime=roots[0];report['runtime']=str(runtime)
        index=json.loads((runtime/'BUNDLE-MANIFEST.json').read_text('utf-8'))
        entries={name:blob['Sha256'] for blob in index['Blobs'] for name in blob['Paths']}
        expected_assets={'image-native/dvda-image.dll','image-native/colors.xml','image-native/policy.xml',
            'image-native/type.xml','menu-bin/dvda-author-dev.exe','menu-bin/fonts/DvdaNotoCJK-Regular.ttc',
            'data/menu/activeheader','data/menu/silence.wav','data/menu/black_PAL_720x576.jpg',
            'data/menu/black_PAL_720x576.png','data/menu/black_NTSC_720x480.jpg','data/menu/black_NTSC_720x480.png'}
        check('EXE embeds only required runtime assets',
            {name for name in entries if not (name.startswith('menu-bin/') and name.endswith('.dll'))}==expected_assets)
        check('Every component matches its embedded hash',all(sha(runtime/name)==h for name,h in entries.items()))
        check('Explicit env import preserves Unicode settings',settings['Values']['DVDA_TITLE']=='独立发布 中文 日本語')
        check('Bundled author and assets automatically located',Path(settings['Values']['DVDA_AUTHOR'])==runtime/'menu-bin/dvda-author-dev.exe' and Path(settings['Values']['DVDA_AUTHOR_SRC'])==runtime/'data')
        stamps={name:(runtime/name).stat().st_mtime_ns for name in entries}
        smoke('warm')
        check('Warm launch reuses verified cache',all((runtime/name).stat().st_mtime_ns==stamp for name,stamp in stamps.items()))
        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
            list(pool.map(smoke,['parallel-1','parallel-2','parallel-3']))
        check('Concurrent GUI launches succeed',len(list(cache.iterdir()))==1)
        dll='menu-bin/dvda-media.dll';missing='data/menu/activeheader'
        (runtime/dll).unlink();(runtime/dll).write_bytes(b'damaged');(runtime/missing).unlink()
        smoke('repair')
        check('Missing and corrupt files automatically repaired',all(sha(runtime/name)==h for name,h in entries.items()))
        check('Repair keeps healthy files unchanged',all((runtime/name).stat().st_mtime_ns==stamp for name,stamp in stamps.items() if name not in (dll,missing)))
        dlls=[Path(name).name.lower() for name in entries if name.lower().endswith('.dll')]
        check('Exactly one physical file for every DLL name',len(dlls)==len(set(dlls)) and not (runtime/'media-native').exists())
        media=json.loads((provenance/'menu-bin/media-build.json').read_text('utf-8'))
        author=json.loads((provenance/'image-native/author-build.json').read_text('utf-8'))
        expected_runtime_dlls=set(media['files'])|set(author['runtime_files'])
        if (runtime/'menu-bin/dvda-formats.dll').exists(): expected_runtime_dlls.add('dvda-formats.dll')
        check('Runtime DLLs exactly match validated dependency manifests',
            {Path(name).name for name in entries if name.startswith('menu-bin/') and name.endswith('.dll')}==expected_runtime_dlls)
        check('Both consumers built against the shared source profile',media['profile']=='shared' and author['ffmpeg_profile']=='build-minimal-ffmpeg.py:shared')
        common=set(media['files'])&set(author['runtime_files'])
        check('Shared FFmpeg identities match both build manifests',{'avcodec-63.dll','avformat-63.dll','avutil-61.dll'}<=common and all(media['files'][name]['sha256']==author['runtime_files'][name]['sha256']==sha(runtime/'menu-bin'/name) for name in common))
        settings['Values']['DVDA_AUTHOR']=str(cache/('0'*64)/'menu-bin/dvda-author-dev.exe')
        settings['Values']['DVDA_AUTHOR_SRC']=str(cache/('0'*64)/'data')
        old=work/'old-profile.json';old.write_text(json.dumps(settings,ensure_ascii=False),encoding='utf-8')
        upgraded=smoke('profile-upgrade',old)
        check('Saved profiles follow the current runtime version',Path(upgraded['Values']['DVDA_AUTHOR'])==runtime/'menu-bin/dvda-author-dev.exe' and Path(upgraded['Values']['DVDA_AUTHOR_SRC'])==runtime/'data')
        check('Only the EXE is needed beside the application',list(release.iterdir())==[executable])
        check('No bundled CLR or developer CLI',not any(Path(name).name.lower() in ['coreclr.dll','hostfxr.dll','dvda.exe','dvda.dll'] for name in entries))
        check('No temporary extraction files remain',not list(cache.rglob('*.tmp')))
        previous=a.previous.resolve()
        report['size']={'exe':executable.stat().st_size,'release_zip':archive.stat().st_size,'previous_zip':previous.with_suffix('.zip').stat().st_size,
            'previous_directory':sum(f.stat().st_size for f in previous.rglob('*') if f.is_file()),
            'cache_logical':sum((runtime/name).stat().st_size for name in entries),
            'cache_unique':sum(blob['Length'] for blob in index['Blobs'])}
        report['sha256']=sha(executable)
        check('EXE smaller than the previous ZIP',report['size']['exe']<report['size']['previous_zip'])
        if not a.startup_only:
            scripts=Path(__file__).parent
            for name,script,extra in [('gui','test-image-release.py',['--baseline',str(a.baseline.resolve()),'--provenance',str(provenance)]),('workflows','test-menu-workflows.py',[])]:
                command=[sys.executable,'-B',str(scripts/script),'--package',str(runtime),'--onefile',str(executable),
                    '--fixtures',str(a.fixtures.resolve()),'--output',str(work/name),*extra]
                with (work/(name+'.log')).open('wb') as log:
                    result=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,timeout=1800)
                check(name+' full regression',result.returncode==0)
                report[name]=json.loads((work/name/'report.json').read_text('utf-8'))
        report['status']='PASS'
    except Exception as e:report.update(status='FAIL',error=str(e));raise
    finally:
        (work/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
        print(json.dumps({'status':report['status'],'checks':len(report['checks']),'report':str(work/'report.json')}),flush=True)

if __name__=='__main__':main()
