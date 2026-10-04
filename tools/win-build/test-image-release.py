"""Exercise an isolated GUI image release, including the full child process tree."""
from pathlib import Path
import argparse, ctypes, hashlib, importlib.util, json, os, shutil, subprocess, sys, time, zipfile

repo=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(repo/'tools/win-build/native'))
from pe_dependencies import Pe
spec=importlib.util.spec_from_file_location('trace',Path(__file__).with_name('trace-child-processes.py'))
trace=importlib.util.module_from_spec(spec);spec.loader.exec_module(trace)

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def short(path):
    buffer=ctypes.create_unicode_buffer(32768)
    if not ctypes.windll.kernel32.GetShortPathNameW(str(path),buffer,len(buffer)):raise ctypes.WinError()
    return buffer.value.replace('\\','/')

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--package',type=Path,required=True)
    p.add_argument('--baseline',type=Path,required=True);p.add_argument('--fixtures',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);p.add_argument('--index',action='store_true')
    p.add_argument('--single-index',action='store_true')
    p.add_argument('--onefile',type=Path,help='Launch this isolated EXE; --package is its extracted runtime')
    p.add_argument('--provenance',type=Path,help='External component build records for onefile packages')
    a=p.parse_args()
    if a.index and a.single_index:p.error('Choose one index fixture.')
    after=a.package.resolve();before=a.baseline.resolve();fixtures=a.fixtures.resolve();work=a.output.resolve();work.mkdir(parents=True,exist_ok=False)
    provenance=a.provenance.resolve() if a.provenance else after
    env={k:v for k,v in os.environ.items() if not k.upper().startswith(('DVDA_','MAGICK_','DOTNET_','COREHOST_','FONTCONFIG_'))}
    env['PATH']=os.pathsep.join([os.environ['SystemRoot']+'/System32',os.environ['SystemRoot'],str(Path(shutil.which('dotnet')).parent)])
    executable=a.onefile.resolve() if a.onefile else after/'DVD-Audio-Maker.exe'
    if a.onefile:env['DVDA_BUNDLE_CACHE_ROOT']=str(after.parent)
    report={'status':'RUNNING','checks':[],'processes':[],'pixels':[]}
    def check(name,ok):
        if not ok:raise AssertionError(name)
        report['checks'].append(name);print('PASS '+name,flush=True)
    def gui(name,config,action):
        extra={'DVDA_GUI_SMOKE_DIRECTORY':str(work),'DVDA_GUI_SMOKE_RAW_OUTPUT':str(work/(name+'.log')),'DVDA_GUI_SMOKE_ACTION':action}
        result,processes=trace.run_traced([executable,'--smoke-test','--config',config,'--language','en'],work,env|extra,600)
        (work/(name+'.stdout')).write_bytes(result.stdout);(work/(name+'.stderr')).write_bytes(result.stderr)
        report['processes'].extend(processes)
        check(name+' GUI success (see '+name+'.log)',result.returncode==0)
    def profile(path):
        return dict((k,v.strip('"')) for line in path.read_text('utf-8').splitlines() if '=' in line and not line.startswith('#') for k,v in [line.split('=',1)])
    def save(path,values):path.write_text(''.join(f'{k}="{v}"\n' for k,v in values.items()),encoding='utf-8')
    try:
        check('No external media or ImageMagick executables bundled',not any(p.name.lower() in ['ffmpeg.exe','ffprobe.exe','magick.exe','convert.exe','mogrify.exe','identify.exe'] for p in after.rglob('*.exe')))
        check('No legacy mkisofs executable bundled', not (after/'menu-bin/mkisofs.exe').exists())
        check('Unused spuunmux reverse parser is omitted', not (after/'menu-bin/spuunmux.exe').exists())
        check('In-process menu image conversion replaces jpeg2yuv', not (after/'menu-bin/jpeg2yuv.exe').exists())
        image_runtime=ctypes.WinDLL(str(after/'image-native/dvda-image.dll'))
        command=image_runtime.dvda_image_command;command.argtypes=[ctypes.c_char_p];command.restype=ctypes.c_int
        source_image=work/'y4m-red.png';y4m=work/'y4m-red.y4m'
        command_line=f'convert -size 4x4 xc:red "{source_image.as_posix()}"'.encode('utf-8')
        check('Image fixture created through the bundled in-process runtime',command(command_line)==0 and source_image.is_file())
        write_y4m=image_runtime.dvda_image_write_y4m
        write_y4m.argtypes=[ctypes.c_char_p,ctypes.c_char_p,ctypes.c_char_p,ctypes.c_char_p]
        write_y4m.restype=ctypes.c_int
        check('ImageMagick bridge writes DVD menu YUV4MPEG2',write_y4m(str(source_image).encode('utf-8'),str(y4m).encode('utf-8'),b'25',b'4:3')==0)
        header,frame=y4m.read_bytes().split(b'\nFRAME\n',1)
        check('YUV4MPEG2 header, frame size and BT.601 conversion',header==b'YUV4MPEG2 W4 H4 F25:1 Ip A4:3 C420jpeg' and len(frame)==24 and frame[:16]==bytes([82])*16 and frame[16:20]==bytes([90])*4 and frame[20:]==bytes([240])*4)
        check('MLP core unchanged',sha(repo/'src/DvdaMaker.SurcodeTool/Native/win-x64/mlp_encoder.dll')=='ece6d0a8033a26e2528042a7b74c66c249ea3c8d7378c06809fb94c8f6bd79b8')
        media_dir=after/('menu-bin' if (after/'menu-bin/dvda-media.dll').exists() else 'media-native')
        media_record=json.loads((provenance/media_dir.name/'media-build.json').read_text('utf-8'))
        if media_record.get('profile')=='shared':
            check('Shared media runtime matches its build manifest',all(sha(media_dir/name)==item['sha256'] for name,item in media_record['files'].items()))
            check('GUI and author use one physical FFmpeg set',media_dir==after/'menu-bin' and not (after/'media-native').exists())
        else:check('Audio/media libraries unchanged',all((after/'media-native'/x.name).read_bytes()==x.read_bytes() for x in (before/'media-native').glob('*.dll')))
        check('Fonts remain shared and unchanged',(after/'menu-bin/fonts/DvdaNotoCJK-Regular.ttc').read_bytes()==(before/'menu-bin/fonts/DvdaNotoCJK-Regular.ttc').read_bytes())
        check('No bundled .NET or developer CLI',not any((after/x).exists() for x in ['coreclr.dll','hostfxr.dll','dvda.exe','dvda.dll']))
        manifest_name='BUNDLE-MANIFEST.json' if a.onefile else 'MANIFEST.txt'
        if a.onefile:
            bundle=json.loads((after/manifest_name).read_text('utf-8'))
            entries={name:blob['Sha256'] for blob in bundle['Blobs'] for name in blob['Paths']}
        else:entries={line.split('  ',1)[1]:line.split('  ',1)[0] for line in (after/manifest_name).read_text().splitlines() if line and not line.startswith('#')}
        check('Package hashes verified',all(sha(after/name)==value for name,value in entries.items()))
        check('Package manifest complete',set(entries)=={p.relative_to(after).as_posix() for p in after.rglob('*') if p.is_file() and p.name!=manifest_name})
        author_record=json.loads((provenance/'image-native/author-build.json').read_text(encoding='utf-8'))
        check('Native DLLs exactly match both validated runtime manifests',
            {x.name.lower() for x in (after/'menu-bin').glob('*.dll')} ==
            ({name.lower() for name in author_record['runtime_files']} |
             ({name.lower() for name in media_record['files']} if media_dir==after/'menu-bin' else set())))
        author_imports=Pe(after/'menu-bin/dvda-author-dev.exe').imports()
        check('Author dynamically links the source-built menu/shared profile',author_record.get('ffmpeg_linkage') in ['shared-source-built-menu-profile','shared-source-built-shared-profile'] and {'avcodec-63.dll','avformat-63.dll','avutil-61.dll'} <= set(author_imports))
        check('Menu media, subpicture and navigation tools are in process',not any((after/'menu-bin'/name).exists() for name in ['mpeg2enc.exe','mplex.exe','mp2enc.exe','spumux.exe','dvdauthor.exe']))
        check('Source-built menu and full-disc verification DLLs are bundled',all((after/'menu-bin'/name).is_file() for name in ['dvda-menu-spu.dll','dvda-menu-nav.dll','dvda-disc-verify.dll']))
        values=profile(fixtures/'after-menu.env');values.pop('DVDA_MKISOFS', None);values.update({'DVDA_BUILD_DIR':str(work/'disc-build'),'DVDA_FINAL_DIR':str(work/'isos'),
            'DVDA_AUTHOR':short(after/'menu-bin/dvda-author-dev.exe'),'DVDA_AUTHOR_SRC':short(after/'data'),
            'DVDA_MENU_FONT':'','DVDA_MENU_FONT_JP':'','DVDA_MENU_FONT_KR':'','DVDA_FFMPEG':str(work/'unavailable-ffmpeg.exe'),
            'DVDA_FFPROBE':str(work/'unavailable-ffprobe.exe'),'DVDA_RESUME':'off'})
        indexed = a.index or a.single_index
        if indexed:
            music=work/'music';music.mkdir()
            albums=sorted((fixtures/'music').iterdir())
            for i in range(7 if a.index else 3):
                album=music/f'{i:02d}-{albums[i%len(albums)].name}'
                shutil.copytree(albums[i%len(albums)],album)
                for file in album.glob('*.flac'):file.rename(file.with_name(f'{i:02d}-'+file.name))
            values.update(DVDA_SRC=str(music),DVDA_MENU_INDEX_MIN_ALBUMS='2')
        else:values['DVDA_SRC']=str(fixtures/'music')
        config=work/'project.env';save(config,values)
        if a.onefile:
            # Exercise bundled defaults instead of supplying paths into the cache.
            values.update(DVDA_AUTHOR='',DVDA_AUTHOR_SRC='');save(config,values)
        gui('prepare',config,'Prepare');gui('build',config,'Build');gui('verify',config,'Verify')
        check('Complete ISO produced',len(list((work/'isos').glob('*.iso')))==1)
        forbidden={'ffmpeg.exe','ffprobe.exe','magick.exe','convert.exe','mogrify.exe','identify.exe','jpeg2yuv.exe','mpeg2enc.exe','mplex.exe','mp2enc.exe','surcodemlp.exe','spumux.exe','dvdauthor.exe','mkisofs.exe','metaflac.exe'}
        check('Process trace contains no external image/media/MLP encoder',not any(Path(x['path']).resolve().name.lower() in forbidden for x in report['processes']))
        check('Native author actually ran',any(Path(x['path']).resolve().name.lower()=='dvda-author-dev.exe' for x in report['processes']))
        expected={x.relative_to(fixtures/'after-menu/mlp').as_posix():sha(x) for x in (fixtures/'after-menu/mlp').rglob('*.mlp')}
        actual={x.relative_to(work/'disc-build/mlp').as_posix():sha(x) for x in (work/'disc-build/mlp').rglob('*.mlp')}
        if not indexed:check('Six whole MLP outputs identical',len(actual)==6 and actual==expected)
        else:check('Index fixture MLP output identities preserved',len(actual)==(14 if a.index else 6) and set(actual.values())<=set(expected.values()))
        images=lambda root:{x.relative_to(root).as_posix():x for name in ['menu','tmp'] for x in (root/name).rglob('*') if x.suffix.lower() in ['.png','.jpg']}
        if not indexed:
            old=images(fixtures/'after-menu');new=images(work/'disc-build')
            check('Same menu/still image set',old.keys()==new.keys() and len(new)>10)
            for name in old:
                pixels=[]
                for file in [old[name],new[name]]:
                    r=subprocess.run([str(before/'menu-bin/magick.exe'),str(file),'-depth','8','RGBA:-'],capture_output=True,env=env,creationflags=subprocess.CREATE_NO_WINDOW,check=True)
                    pixels.append(r.stdout)
                check('Matching image dimensions '+name,len(pixels[0])==len(pixels[1]))
                record={'name':name,'equal':pixels[0]==pixels[1]}
                if not record['equal']:
                    differences=[abs(x-y) for x,y in zip(*pixels)]
                    record.update(max_delta=max(differences),mean_delta=sum(differences)/len(differences),changed_channels=sum(x!=0 for x in differences))
                report['pixels'].append(record)
        if a.onefile:
            report['size']={'exe':executable.stat().st_size,'cache_logical':sum(x.stat().st_size for x in after.rglob('*') if x.is_file())}
        else:
            report['size']={key:{'zip':root.with_suffix('.zip').stat().st_size,'unpacked':sum(x.stat().st_size for x in root.rglob('*') if x.is_file())} for key,root in [('before',before),('after',after)]}
            check('ZIP and extracted package both smaller',all(report['size']['after'][k]<report['size']['before'][k] for k in ['zip','unpacked']))
        report['status']='PASS'
    except Exception as error:report.update(status='FAIL',error=str(error));raise
    finally:
        (work/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
        print(json.dumps({'status':report['status'],'checks':len(report['checks']),'report':str(work/'report.json')}),flush=True)

if __name__=='__main__':main()
