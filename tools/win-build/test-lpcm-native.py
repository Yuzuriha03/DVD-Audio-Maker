"""Small deterministic LPCM authoring matrix; no music-library production run."""
from pathlib import Path
import argparse, ctypes, hashlib, json, os, struct, subprocess

def wave(path,rate,bits,channels,frames,seed):
    width=bits//8; mask=[4,3,7,0x33,0x37,0x3f][channels-1]
    # Distinct channel values, signs, low bytes, boundary values and odd lengths.
    samples=[((i*7919+c*32771+seed*104729)%(1<<bits)) for i in range(frames) for c in range(channels)]
    data=b''.join(value.to_bytes(width,'little') for value in samples)
    fmt=struct.pack('<HHIIHHHHI',0xfffe,channels,rate,rate*channels*width,channels*width,bits,22,bits,mask)+bytes.fromhex('0100000000001000800000aa00389b71')
    body=b'WAVEfmt '+struct.pack('<I',40)+fmt+b'data'+struct.pack('<I',len(data))+data+b'\0'*(len(data)%2)
    path.write_bytes(b'RIFF'+struct.pack('<I',len(body))+body)

class Result(ctypes.Structure):
    _fields_=[('code',ctypes.c_int),('track',ctypes.c_int),('offset',ctypes.c_uint64),('sectors',ctypes.c_uint64),('bytes',ctypes.c_uint64)]
Read=ctypes.CFUNCTYPE(ctypes.c_int,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_uint)
def verify(dll,paths,ends,payload):
    at=0
    def read(_,target,capacity):
        nonlocal at
        data=payload[at:at+capacity];at+=len(data)
        if data:ctypes.memmove(target,data,len(data))
        return len(data)
    callback=Read(read);result=Result()
    fn=dll.dvda_verify_lpcm_payload
    fn.argtypes=[ctypes.POINTER(ctypes.c_char_p),ctypes.POINTER(ctypes.c_ubyte),ctypes.c_uint,Read,ctypes.c_void_p,ctypes.POINTER(Result)]
    fn.restype=ctypes.c_int
    status=fn((ctypes.c_char_p*len(paths))(*[str(p).encode('utf-8') for p in paths]),(ctypes.c_ubyte*len(ends))(*ends),len(paths),callback,None,ctypes.byref(result))
    return status,{name:getattr(result,name) for name,_ in result._fields_}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--author',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--quick',action='store_true');a=p.parse_args()
    root=a.output.resolve();root.mkdir(parents=True,exist_ok=False);author=a.author.resolve();dll=ctypes.CDLL(str(author.parent/'dvda-disc-verify.dll'))
    report={'status':'RUNNING','checks':[]};env=os.environ.copy()
    formats=[(r,b,c) for r in [44100,48000,88200,96000,176400,192000] for b in [16,24] for c in range(1,7) if r*b*c<=9600000 and (r<=96000 or c<=2)]
    if a.quick:formats=[(48000,16,2),(48000,24,6),(192000,24,2)]
    try:
        for rate,bits,channels in formats:
            for separate in [False,True]:
                name=f'{rate}-{bits}-{channels}-'+('titles' if separate else 'gapless');work=root/name;work.mkdir();sources=[]
                for i,frames in enumerate([rate//8+1,rate//8+3]):
                    source=work/f'{i}.wav';wave(source,rate,bits,channels,frames,i+1);sources.append(source)
                out=work/'disc';tmp=work/'tmp';tmp.mkdir()
                command=[str(author),'-g',str(sources[0]),*(['-z'] if separate else []),str(sources[1]),'-o',str(out),'-D',str(tmp),'-W','-P0','-n']
                with (work/'author.log').open('wb') as log:
                    r=subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=90,creationflags=subprocess.CREATE_NO_WINDOW)
                if r.returncode:raise AssertionError(name+' author exit '+str(r.returncode))
                segments=sorted((out/'AUDIO_TS').glob('ATS_01_*.AOB'));payload=b''.join(p.read_bytes() for p in segments)
                status,detail=verify(dll,sources,[int(separate),1],payload)
                if status:raise AssertionError(name+' '+json.dumps(detail))
                if not detail['bytes']:raise AssertionError(name+' no audio compared')
                # Real authored data: reject payload mutation, wrong header and truncation.
                changed=bytearray(payload)
                q=next(i for i in range(14,200) if changed[i:i+4]==b'\0\0\1\xbd');private=q+9+changed[q+8];audio=private+4+changed[private+3]
                changed[audio]^=1
                if verify(dll,sources,[int(separate),1],bytes(changed))[0]!=-5:raise AssertionError(name+' corruption accepted')
                changed=bytearray(payload);changed[private+7]^=0x20
                if verify(dll,sources,[int(separate),1],bytes(changed))[0]!=-8:raise AssertionError(name+' wrong bits accepted')
                if verify(dll,sources,[int(separate),1],payload[:-2048])[0]==0:raise AssertionError(name+' truncation accepted')
                report['checks'].append({'case':name,**detail});print('PASS '+name,flush=True)
        report['status']='PASS'
    except Exception as error:report.update(status='FAIL',error=str(error));raise
    finally:(root/'report.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
if __name__=='__main__':main()
