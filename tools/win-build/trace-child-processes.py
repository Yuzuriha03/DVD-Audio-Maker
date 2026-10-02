"""Run a Windows test command and record its complete debug child-process tree.

Uses DEBUG_PROCESS on a newly started test process; never attaches to user apps.
"""
import argparse, concurrent.futures, ctypes, json, os, subprocess, time, sys
from pathlib import Path
from ctypes import wintypes

def run_traced(arguments, cwd, env, timeout=420):
    kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    class Payload(ctypes.Union):
        _fields_=[('data',ctypes.c_ubyte*160),('align',ctypes.c_uint64)]
    class DebugEvent(ctypes.Structure):
        _fields_=[('code',wintypes.DWORD),('process',wintypes.DWORD),('thread',wintypes.DWORD),('payload',Payload)]
    kernel.WaitForDebugEvent.argtypes=[ctypes.POINTER(DebugEvent),wintypes.DWORD]
    kernel.ContinueDebugEvent.argtypes=[wintypes.DWORD,wintypes.DWORD,wintypes.DWORD]
    kernel.QueryFullProcessImageNameW.argtypes=[wintypes.HANDLE,wintypes.DWORD,wintypes.LPWSTR,ctypes.POINTER(wintypes.DWORD)]
    kernel.CloseHandle.argtypes=[wintypes.HANDLE]
    kernel.TerminateProcess.argtypes=[wintypes.HANDLE,wintypes.UINT]
    process=subprocess.Popen([str(x) for x in arguments],cwd=cwd,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,
        creationflags=0x1|subprocess.CREATE_NO_WINDOW)
    children=[];live=set();handles={};deadline=time.monotonic()+timeout
    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
        output=pool.submit(process.communicate)
        while True:
            if time.monotonic()>deadline:
                for handle in handles.values():kernel.TerminateProcess(handle,124)
                process.kill()
                deadline=time.monotonic()+10
            event=DebugEvent()
            if not kernel.WaitForDebugEvent(ctypes.byref(event),1000):
                if output.done() and not live:break
                continue
            raw=bytes(event.payload.data);status=0x10002
            handle=lambda offset:int.from_bytes(raw[offset:offset+8],'little')
            if event.code==3:
                file,proc,thread=handle(0),handle(8),handle(16)
                name=ctypes.create_unicode_buffer(32768);size=wintypes.DWORD(len(name))
                if not kernel.QueryFullProcessImageNameW(proc,0,name,ctypes.byref(size)):raise ctypes.WinError(ctypes.get_last_error())
                children.append({'pid':event.process,'path':name.value});live.add(event.process);handles[event.process]=proc
                if file:kernel.CloseHandle(file)
                # The system closes debug process/thread handles at their EXIT
                # events. Closing them here can double-close a reused handle.
            elif event.code==6:
                if handle(0):kernel.CloseHandle(handle(0))
            elif event.code==5:
                live.discard(event.process);handles.pop(event.process,None)
            elif event.code==1:
                code=int.from_bytes(raw[:4],'little')
                if code in [0xc0000409,0xc0000005]:
                    print('NATIVE_EXCEPTION '+json.dumps({'code':hex(code),'address':hex(handle(16)),
                        'parameters':[hex(handle(32+i*8)) for i in range(min(int.from_bytes(raw[24:28],'little'),15))]}),flush=True)
                if code not in [0x80000003,0x80000004,0x40010006,0x4001000A]:status=0x80010001
            if not kernel.ContinueDebugEvent(event.process,event.thread,status):raise ctypes.WinError(ctypes.get_last_error())
            if not live and output.done():break
        stdout,stderr=output.result()
    return subprocess.CompletedProcess(arguments,process.returncode,stdout,stderr),children

if __name__=='__main__':
    sys.stdout.reconfigure(encoding='utf-8',errors='replace')
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--report',required=True);p.add_argument('command',nargs=argparse.REMAINDER)
    a=p.parse_args();result,children=run_traced(a.command,Path.cwd(),os.environ)
    Path(a.report).write_text(json.dumps({'exit_code':result.returncode,'processes':children},indent=2),encoding='utf-8')
    print(result.stdout.decode('utf-8','replace'));print(result.stderr.decode('utf-8','replace'));raise SystemExit(result.returncode)
