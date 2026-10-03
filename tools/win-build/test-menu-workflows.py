"""Exercise multi-disc/group verification and converted PCM through the GUI."""
from pathlib import Path
import argparse, importlib.util, json, os, shutil

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--package',type=Path,required=True)
    p.add_argument('--fixtures',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();package=a.package.resolve();fixtures=a.fixtures.resolve();out=a.output.resolve()
    out.mkdir(parents=True,exist_ok=False)
    spec=importlib.util.spec_from_file_location('release',Path(__file__).with_name('test-image-release.py'))
    helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
    env={k:v for k,v in os.environ.items() if not k.upper().startswith(('DVDA_','MAGICK_','DOTNET_','COREHOST_','FONTCONFIG_'))}
    env['PATH']=os.pathsep.join([os.environ['SystemRoot']+'/System32',os.environ['SystemRoot'],str(Path(shutil.which('dotnet')).parent)])
    base=dict((k,v.strip('"')) for line in (fixtures/'after-menu.env').read_text('utf-8').splitlines() if '=' in line and not line.startswith('#') for k,v in [line.split('=',1)])
    base.update(DVDA_SRC=str(fixtures/'music'),DVDA_AUTHOR=helper.short(package/'menu-bin/dvda-author-dev.exe'),
                DVDA_AUTHOR_SRC=helper.short(package/'data'),DVDA_MENU_FONT='',DVDA_MENU_FONT_JP='',DVDA_MENU_FONT_KR='',
                DVDA_FFMPEG=str(out/'missing-ffmpeg.exe'),DVDA_FFPROBE=str(out/'missing-ffprobe.exe'),DVDA_RESUME='off')
    report={'status':'RUNNING','checks':[],'processes':[]}
    def check(name,ok):
        if not ok:raise AssertionError(name)
        report['checks'].append(name);print('PASS '+name,flush=True)
    try:
        for name,extra in [('no-menu-multidisc',{'DVDA_MENU':'off','DVDA_GROUP_TRACK_LIMIT':'2','DVDA_DISC_BYTES':'11000000','DVDA_MAX_DISCS':'3'}),
                           ('single-index-20bit',{'DVDA_MENU':'on','DVDA_MENU_INDEX_MIN_ALBUMS':'2','DVDA_MLP_SURCODE_SAMPLE_RATE':'44100','DVDA_MLP_SURCODE_BITS':'20'})]:
            work=out/name;work.mkdir();config=work/'project.env'
            values=base|{'DVDA_BUILD_DIR':str(work/'disc-build'),'DVDA_FINAL_DIR':str(work/'isos')}|extra
            config.write_text(''.join(f'{k}="{v}"\n' for k,v in values.items()),encoding='utf-8')
            for action in ['Build','Verify']:
                runenv=env|{'DVDA_GUI_SMOKE_DIRECTORY':str(work),'DVDA_GUI_SMOKE_RAW_OUTPUT':str(work/(action+'.log')),'DVDA_GUI_SMOKE_ACTION':action}
                result,processes=helper.trace.run_traced([package/'DVD-Audio-Maker.exe','--smoke-test','--config',config,'--language','en'],work,runenv,600)
                (work/(action+'.stdout')).write_bytes(result.stdout);(work/(action+'.stderr')).write_bytes(result.stderr)
                report['processes']+=processes;check(name+' '+action,result.returncode==0)
            index=json.loads((work/'disc-build/mlp_index.json').read_text('utf-8'))
            discs=index['__discs__'];tracks=[t for d in discs for g in d['groups'] for t in g['tracks']]
            check(name+' verifies all six tracks',len(tracks)==6)
            if name.startswith('no-menu'):
                check('More than one disc is built and verified',len(discs)>1)
                check('More than one audio group is built and verified',any(len(d['groups'])>1 for d in discs))
            else:
                check('Converted 44.1 kHz / 20-bit targets used',all(index[t['mlp']]['sr']==44100 and index[t['mlp']]['bits']==20 for t in tracks))
        forbidden={'ffmpeg.exe','ffprobe.exe','magick.exe','mogrify.exe','convert.exe','identify.exe','mpeg2enc.exe','mplex.exe','mp2enc.exe','spumux.exe','dvdauthor.exe','surcodemlp.exe','mkisofs.exe','metaflac.exe'}
        check('No external tool process in either workflow',not any(Path(p['path']).name.lower() in forbidden for p in report['processes']))
        report['status']='PASS'
    except Exception as e:report.update(status='FAIL',error=str(e));raise
    finally:(out/'report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')

if __name__=='__main__':main()
