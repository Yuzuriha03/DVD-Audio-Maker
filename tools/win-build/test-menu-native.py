"""Compare the migrated C menu algorithms against captured legacy outputs."""
from pathlib import Path
import argparse,ctypes,hashlib,json,shutil

class Request(ctypes.Structure):
    _fields_=[('size',ctypes.c_uint),('xml',ctypes.c_char_p),('input',ctypes.c_char_p),('output',ctypes.c_char_p),('pixels',ctypes.c_void_p)]

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--runtime',type=Path,default=Path('build/menu-native'))
    p.add_argument('--image-runtime',type=Path,default=Path('build/image-native'))
    p.add_argument('--fixtures',type=Path,required=True)
    p.add_argument('--reference-disc',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
    image=ctypes.CDLL(str((a.image_runtime/'dvda-image.dll').resolve()))
    pixels=ctypes.cast(image.dvda_image_read_rgba,ctypes.c_void_p)
    release=ctypes.windll.kernel32.FreeLibrary;release.argtypes=[ctypes.c_void_p];release.restype=ctypes.c_int
    checks=[];report={'status':'RUNNING','checks':checks}
    def check(name,ok):
        if not ok:raise AssertionError(name)
        checks.append(name);print('PASS '+name,flush=True)
    def run(kind,xml,output,input=None):
        lib=ctypes.CDLL(str((a.runtime/f'dvda-menu-{kind}.dll').resolve()))
        entry=lib.dvda_menu_run;entry.argtypes=[ctypes.POINTER(Request)];entry.restype=ctypes.c_int
        enc=lambda x:str(x).encode('utf-8') if x else None
        req=Request(ctypes.sizeof(Request),enc(xml),enc(input),enc(output),pixels)
        try:return entry(ctypes.byref(req))
        finally:release(lib._handle)
    try:
        for xml in sorted(a.fixtures.glob('spu_xmltemp_*.xml')):
            number=xml.stem.rsplit('_',1)[1];target=out/('topmenu'+number)
            check('Subpicture '+number,run('spu',xml,target,a.fixtures/f'background_movie_{number}.mpg')==0)
            check('Byte-identical subpicture '+number,target.read_bytes()==(a.fixtures/target.name).read_bytes())
        nav=out/'nav';nav.mkdir();(nav/'AUDIO_TS').mkdir();(nav/'VIDEO_TS').mkdir()
        check('AMGM authoring',run('nav',a.fixtures/'xmltemp',nav)==0)
        check('AMGM output exists',(nav/'AUDIO_TS/AUDIO_TS.VOB').is_file())
        for file in nav.rglob('*'):
            if file.is_file():
                reference=a.reference_disc/file.relative_to(nav)
                check('Byte-identical navigation '+file.name,reference.is_file() and file.read_bytes()==reference.read_bytes())
        invalid=out/'bad.xml';invalid.write_text('<subpictures><stream><unknown/></stream></subpictures>')
        sample=a.fixtures/'background_movie_0.mpg'
        check('Malformed XML returns failure',run('spu',invalid,out/'failed.mpg',sample)!=0)
        invalid.write_text('<!DOCTYPE subpictures [<!ENTITY x SYSTEM "file:///missing">]><subpictures>&x;</subpictures>')
        check('DTD returns failure',run('spu',invalid,out/'failed.mpg',sample)!=0)
        check('Missing MPEG returns failure',run('spu',a.fixtures/'spu_xmltemp_0.xml',out/'failed.mpg',out/'missing.mpg')!=0)
        truncated=out/'truncated.mpg';truncated.write_bytes(sample.read_bytes()[:-1])
        check('Truncated MPEG returns failure',run('spu',a.fixtures/'spu_xmltemp_0.xml',out/'failed.mpg',truncated)!=0)
        check('Null input returns failure',run('spu',a.fixtures/'spu_xmltemp_0.xml',out/'failed.mpg')!=0)
        for i in range(12):
            target=out/'repeated.mpg'
            check('Repeat after failure '+str(i),run('spu',a.fixtures/'spu_xmltemp_0.xml',target,sample)==0 and target.read_bytes()==(a.fixtures/'topmenu0').read_bytes())
        report['status']='PASS'
    except Exception as e:report.update(status='FAIL',error=str(e));raise
    finally:(out/'report.json').write_text(json.dumps(report,indent=2)+'\n')

if __name__=='__main__':main()
