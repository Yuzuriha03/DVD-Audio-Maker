"""Check full-stream AOB comparison, including corruption after the first track."""
from pathlib import Path
import argparse, ctypes, hashlib, json

class Result(ctypes.Structure):
    _fields_=[('code',ctypes.c_int),('track',ctypes.c_int),('offset',ctypes.c_uint64),
              ('sectors',ctypes.c_uint64),('bytes',ctypes.c_uint64)]

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--runtime',type=Path,default=Path('build/menu-native'))
    p.add_argument('--aob-directory',type=Path,required=True)
    p.add_argument('--mlp-directory',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();a.output.mkdir(parents=True,exist_ok=False)
    inputs=sorted(a.mlp_directory.rglob('*.mlp'))
    aobs=sorted(a.aob_directory.glob('ATS_*.AOB'))
    if len(inputs)<2 or not aobs:raise ValueError('Use a real multi-track corpus.')
    hashes={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [*inputs,*aobs]}
    data=b''.join(p.read_bytes() for p in aobs)
    callback=ctypes.CFUNCTYPE(ctypes.c_int,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_uint)
    dll=ctypes.CDLL(str((a.runtime/'dvda-disc-verify.dll').resolve()))
    entry=dll.dvda_verify_mlp_payload
    entry.argtypes=[ctypes.POINTER(ctypes.c_char_p),ctypes.c_uint,callback,ctypes.c_void_p,ctypes.POINTER(Result)]
    entry.restype=ctypes.c_int
    report={'status':'RUNNING','checks':[],'tracks':len(inputs),'source_bytes':sum(p.stat().st_size for p in inputs)}
    def check(name,ok):
        if not ok:raise AssertionError(name)
        report['checks'].append(name);print('PASS '+name,flush=True)
    def run(blob=data,paths=inputs,chunk=128*1024,cancel=False):
        position=0
        @callback
        def read(_,buffer,capacity):
            nonlocal position
            if cancel:return -1
            block=blob[position:position+min(chunk,capacity)]
            if block:ctypes.memmove(buffer,block,len(block))
            position+=len(block);return len(block)
        names=(ctypes.c_char_p*len(paths))(*(str(p.resolve()).encode('utf-8') for p in paths))
        result=Result();status=entry(names,len(paths),read,None,ctypes.byref(result))
        return status,result
    try:
        status,result=run()
        check('All real AOB segments match every ordered MLP byte',status==0 and result.bytes==report['source_bytes'])
        check('One-sector callbacks preserve track and segment boundaries',run(chunk=2048)[0]==0)
        check('Unicode source paths work',any(not str(p).isascii() for p in inputs) and run()[0]==0)
        # Locate a real audio byte near the end; avoid padding and pack headers.
        payload=[]
        for sector in range(0,len(data),2048):
            at=sector+14+(data[sector+13]&7)
            while at+6<=sector+2048 and data[at:at+3]==b'\x00\x00\x01':
                end=at+6+int.from_bytes(data[at+4:at+6],'big')
                if data[at+3]==0xbd:
                    private=at+9+data[at+8];start=private+4+data[private+3]
                    if start<end:payload.append((start,end,at))
                if end<=at or end>sector+2048:break
                at=end
        check('Real PES payloads found',len(payload)>2)
        broken=bytearray(data);broken[payload[-1][0]]^=1
        status,result=run(bytes(broken))
        check('Last-track corruption is detected and located',status==-5 and result.track==len(inputs)-1)
        check('Dropping final source track rejects extra disc bytes',run(paths=inputs[:-1])[0]==-6)
        check('An unrepresented final source track is rejected',run(paths=inputs+[inputs[-1]])[0]==-7)
        check('Missing final audio sectors are detected',run(data[:payload[-1][0]//2048*2048])[0]==-7)
        broken=bytearray(data);broken[payload[-1][2]+4:payload[-1][2]+6]=b'\xff\xff'
        check('Malformed PES length is rejected',run(bytes(broken))[0]==-2)
        check('Partial final sector is rejected',run(data[:-1])[0]==-9)
        check('Cancellation callback stops verification',run(cancel=True)[0]==-9)
        check('Missing source cannot pass',run(paths=[a.output/'missing.mlp',*inputs[1:]])[0]==-4)
        check('A successful call still works after failures',run()[0]==0)
        check('Verification never changes source MLP or AOB',all(hashlib.sha256(Path(p).read_bytes()).hexdigest()==h for p,h in hashes.items()))
        report['status']='PASS'
    except Exception as e:report.update(status='FAIL',error=str(e));raise
    finally:(a.output/'report.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')

if __name__=='__main__':main()
