"""Exercise the actual C menu bridge across PAL/NTSC and invalid input paths."""
from pathlib import Path
import argparse, ctypes, json, math, os, struct, subprocess, wave

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--msys-root',type=Path,default=Path('C:/msys64'))
    p.add_argument('--ffmpeg-runtime',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
    prefix=a.ffmpeg_runtime.resolve();source=Path(__file__).parent/'native/dvda-menu-media.c'
    compiler=a.msys_root.resolve()/'mingw64/bin/gcc.exe';env=os.environ|{'PATH':str(compiler.parent)+os.pathsep+os.environ['PATH']}
    dll=out/'menu-test.dll'
    subprocess.run([str(compiler),'-O2','-shared','-static-libgcc','-Wall','-Wextra','-Werror',
                    '-I'+str(prefix/'include'),str(source),'-L'+str(prefix/'lib'),
                    '-lavformat','-lavcodec','-lavutil','-o',str(dll)],env=env,check=True)
    search=os.add_dll_directory(str(prefix/'bin'))
    gcc_runtime=os.add_dll_directory(str(compiler.parent))
    library=ctypes.CDLL(str(dll));entry=library.dvda_menu_create_mpg
    entry.argtypes=[ctypes.c_char_p]*5;entry.restype=ctypes.c_int
    checks=[];report={'status':'RUNNING','checks':checks}
    def check(name,ok):
        if not ok:raise AssertionError(name)
        checks.append(name);print('PASS '+name,flush=True)
    def invoke(y4m,wav,target,norm='pal',aspect='4:3'):
        return entry(*(str(s).encode('utf-8') if s is not None else None for s in [y4m,wav,target,norm,aspect]))
    def audio(path,rate=48000,channels=2,samples=12000):
        with wave.open(str(path),'wb') as f:
            f.setnchannels(channels);f.setsampwidth(2);f.setframerate(rate)
            f.writeframes(b''.join(struct.pack('<h',int(10000*math.sin(2*math.pi*440*i/rate)))*channels for i in range(samples)))
    try:
        wav=out/'menu.wav';audio(wav)
        for norm,height,fps,ratecode in [('pal',576,'25:1',3),('ntsc',480,'30000:1001',4)]:
            y4m=out/(norm+' 菜单.y4m')
            y4m.write_bytes(f'YUV4MPEG2 W720 H{height} F{fps} Ip A1:1 C420jpeg\nFRAME\n'.encode()+bytes([82])*(720*height)+bytes([90])*(360*height//2)+bytes([240])*(360*height//2))
            for aspect in ['4:3','16:9']:
                for sound in [False,True]:
                    label=f'{norm}-{aspect.replace(":","x")}-{sound}';target=out/(label+'.mpg')
                    check(label+' encodes',invoke(y4m,wav if sound else None,target,norm,aspect)==0)
                    data=target.read_bytes()
                    check(label+' uses complete DVD pack sectors',len(data)>0 and len(data)%2048==0 and all(data[i:i+4]==b'\0\0\1\xba' for i in range(0,len(data),2048)))
                    pos=data.index(b'\0\0\1\xb3')+4
                    check(label+' sequence dimensions/frame rate/aspect',data[pos]<<4|data[pos+1]>>4==720 and (data[pos+1]&15)<<8|data[pos+2]==height and data[pos+3]&15==ratecode and data[pos+3]>>4==(['4:3','16:9'].index(aspect)+2))
                    repeat=out/'repeat.mpg'
                    check(label+' is deterministic',invoke(y4m,wav if sound else None,repeat,norm,aspect)==0 and repeat.read_bytes()==data)
        pal=out/'pal 菜单.y4m';failed=out/'failed.mpg'
        bad=out/'bad.y4m'
        for name,blob in [('truncated',pal.read_bytes()[:-1]),('huge',pal.read_bytes().replace(b'W720',b'W2147483647',1)),('invalid-token',pal.read_bytes().replace(b'W720',b'W720junk',1))]:
            bad.write_bytes(blob);check(name+' Y4M rejected',invoke(bad,None,failed)!=0)
        for name,rate,channels,samples in [('rate',44100,2,12000),('mono',48000,1,12000),('empty',48000,2,0)]:
            badwav=out/(name+'.wav');audio(badwav,rate,channels,samples)
            check(name+' WAV rejected without partial output',invoke(pal,badwav,failed)!=0 and not failed.exists())
        badwav=out/'truncated.wav';badwav.write_bytes(wav.read_bytes()[:-99])
        check('Truncated WAV rejected',invoke(pal,badwav,failed)!=0 and not failed.exists())
        check('PAL/NTSC mismatch rejected',invoke(pal,None,failed,'ntsc')!=0)
        failed.write_bytes(b'keep-existing-file')
        check('Failure before opening output preserves existing file',invoke(out/'missing.y4m',None,failed)!=0 and failed.read_bytes()==b'keep-existing-file')
        report['status']='PASS'
    except Exception as e:report.update(status='FAIL',error=str(e));raise
    finally:
        (out/'report.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');search.close();gcc_runtime.close()

if __name__=='__main__':main()
