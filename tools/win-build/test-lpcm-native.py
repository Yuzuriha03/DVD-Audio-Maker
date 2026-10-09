"""Small deterministic LPCM authoring matrix; no music-library production run."""
from pathlib import Path
import argparse, json, os, struct, subprocess

def wave(path,rate,bits,channels,frames,seed):
    width=bits//8; mask=[4,3,7,0x33,0x37,0x3f][channels-1]
    # Distinct channel values, signs, low bytes, boundary values and odd lengths.
    samples=[((i*7919+c*32771+seed*104729)%(1<<bits)) for i in range(frames) for c in range(channels)]
    data=b''.join(value.to_bytes(width,'little') for value in samples)
    fmt=struct.pack('<HHIIHHHHI',0xfffe,channels,rate,rate*channels*width,channels*width,bits,22,bits,mask)+bytes.fromhex('0100000000001000800000aa00389b71')
    body=b'WAVEfmt '+struct.pack('<I',40)+fmt+b'data'+struct.pack('<I',len(data))+data+b'\0'*(len(data)%2)
    path.write_bytes(b'RIFF'+struct.pack('<I',len(body))+body)

def verify(executable, paths, ends, payload, work):
    aob=work/'verify-input.aob'
    aob.write_bytes(payload)
    args=[str(executable),'verify-aob','--lpcm','--title-ends',','.join(map(str,ends))]
    for source in paths:
        args.extend(['--source',str(source)])
    args.extend(['--aob',str(aob)])
    try:
        result=subprocess.run(args,capture_output=True,text=True,timeout=90)
    finally:
        aob.unlink()
    if result.returncode:
        return result.returncode,result.stderr.strip()
    return 0,json.loads(result.stdout)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--author',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--quick',action='store_true');p.add_argument('--verifier',type=Path,required=True);a=p.parse_args()
    root=a.output.resolve();root.mkdir(parents=True,exist_ok=False);author=a.author.resolve();verifier=a.verifier.resolve()
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
                status,detail=verify(verifier,sources,[int(separate),1],payload,work)
                if status:raise AssertionError(name+' '+json.dumps(detail))
                if not detail['bytes']:raise AssertionError(name+' no audio compared')
                verified=detail
                # Real authored data: reject payload mutation, wrong header and truncation.
                changed=bytearray(payload)
                q=next(i for i in range(14,200) if changed[i:i+4]==b'\0\0\1\xbd');private=q+9+changed[q+8];audio=private+4+changed[private+3]
                changed[audio]^=1
                status,detail=verify(verifier,sources,[int(separate),1],bytes(changed),work)
                if status==0 or 'status -5' not in detail:raise AssertionError(name+' corruption accepted: '+str(detail))
                changed=bytearray(payload);changed[private+7]^=0x20
                status,detail=verify(verifier,sources,[int(separate),1],bytes(changed),work)
                if status==0 or 'status -8' not in detail:raise AssertionError(name+' wrong bits accepted: '+str(detail))
                if verify(verifier,sources,[int(separate),1],payload[:-2048],work)[0]==0:raise AssertionError(name+' truncation accepted')
                report['checks'].append({'case':name,**verified});print('PASS '+name,flush=True)
        for bits, channels in [(16,1),(24,1),(24,2),(24,6)]:
            for frames in [1,2,3,127,501]:
                for separate in [False,True]:
                    name=f'tiny-{bits}-{channels}-{frames}-'+('titles' if separate else 'gapless')
                    work=root/name;work.mkdir();sources=[]
                    for i in range(2):
                        source=work/f'{i}.wav';wave(source,48000,bits,channels,frames,i+1);sources.append(source)
                    out=work/'disc';tmp=work/'tmp';tmp.mkdir()
                    with (work/'author.log').open('wb') as log:
                        r=subprocess.run([str(author),'-g',str(sources[0]),*(['-z'] if separate else []),str(sources[1]),'-o',str(out),'-D',str(tmp),'-W','-P0','-n'],env=env,stdout=log,stderr=subprocess.STDOUT,timeout=90,creationflags=subprocess.CREATE_NO_WINDOW)
                    payload=b''.join(p.read_bytes() for p in sorted((out/'AUDIO_TS').glob('ATS_01_*.AOB')))
                    status,detail=verify(verifier,sources,[int(separate),1],payload,work)
                    if r.returncode or status:raise AssertionError(name+' '+json.dumps(detail))
                    expected_frames=2*(frames+(frames%2)) if separate else 2*frames
                    if detail['bytes']!=expected_frames*channels*(bits//8):raise AssertionError(name+' incorrect PCM length')
                    report['checks'].append({'case':name,**detail});print('PASS '+name,flush=True)
        report['status']='PASS'
    except Exception as error:report.update(status='FAIL',error=str(error));raise
    finally:(root/'report.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
if __name__=='__main__':main()
