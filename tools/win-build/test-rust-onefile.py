"""Run the shipped EXE with generated audio, no development DLL paths or user profile."""
import argparse, ctypes as c, hashlib, importlib.util, json, os, shutil, struct, subprocess, sys, tempfile, time, wave
from pathlib import Path

def generate(runtime, root):
    class Request(c.Structure):
        _fields_ = [(n, c.c_uint32) for n in ("size","abi","operation","rate","bits","format","soxr","compression","cover","tag_count")] + [
            ("input",c.c_char_p),("output",c.c_char_p),("tags",c.POINTER(c.c_char_p))]
    emit_type=c.CFUNCTYPE(None,c.c_void_p,c.c_int,c.c_char_p)
    cancel_type=c.CFUNCTYPE(c.c_int,c.c_void_p)
    dll=c.CDLL(str(runtime/"dvda-media.dll"))
    dll.dvdamedia_run.argtypes=[c.POINTER(Request),emit_type,cancel_type,c.c_void_p]
    root.mkdir()
    logs=[]
    emit=emit_type(lambda _,stream,text: logs.append(text.decode("utf-8","replace")))
    cancel=cancel_type(lambda _:0)
    for n in range(2):
        input=root/f"{n}.wav"
        with wave.open(str(input),"wb") as output:
            output.setparams((2,2,48000,0,"NONE","not compressed"))
            output.writeframes(b"".join(struct.pack("<h",((f*97+ch*113+n*719)%60001)-30000)
                # Keep enough PTS steps for one title reset to stay within the
                # legacy 1% abnormal-step limit; retain a partial final MLP AU.
                for f in range(192013+n*7) for ch in range(2)))
        tags=(c.c_char_p*8)(b"ALBUM",b"Test album",b"TITLE",f"Track {n}".encode(),b"TRACK",str(n+1).encode(),b"DATE",b"2026")
        request=Request(c.sizeof(Request),1,3,0,0,6,0,8,0,4,str(input).encode("utf-8"),
            str(root/f"{n}.flac").encode("utf-8"),tags)
        assert dll.dvdamedia_run(c.byref(request),emit,cancel,None)==0,logs
        input.unlink()
    from PIL import Image
    cover=Image.new("RGB",(320,320))
    cover.putdata([(20+x*200//320,30+y*190//320,40+(x+y)*170//640)
        for y in range(320) for x in range(320)])
    cover.save(root/"cover.png")

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe",type=Path)
    parser.add_argument("--generate",nargs=2,type=Path)
    parser.add_argument("--output",type=Path,default=Path("build/onefile-rust-validation.json"))
    args=parser.parse_args()
    if args.generate:
        generate(*args.generate)
        return
    spec=importlib.util.spec_from_file_location("gui",Path(__file__).with_name("test-rust-gui.py"))
    gui=importlib.util.module_from_spec(spec);spec.loader.exec_module(gui)
    u=gui.u
    with tempfile.TemporaryDirectory(prefix="dvda-onefile-") as temporary:
        root=Path(temporary)
        exe=root/"DVD-Audio-Maker.exe";shutil.copy2(args.exe,exe)
        env={k:v for k,v in os.environ.items() if not k.startswith("DVDA_")}
        env["LOCALAPPDATA"]=str(root/"local")
        profile=root/"settings.json"
        def launch():
            proc=subprocess.Popen([str(exe),"--profile",str(profile)],cwd=root,env=env,
                creationflags=subprocess.CREATE_NO_WINDOW)
            hwnd=gui.wait_for(lambda:gui.find_app(proc.pid),30)
            gui.wait_for(lambda:any(u.GetDlgCtrlID(h)==115 for h in gui.windows(hwnd)))
            gui.wait_for(lambda:any(u.GetDlgCtrlID(h)==200 and gui.text(h)==str(root/"audio") for h in gui.windows(hwnd)))
            controls={u.GetDlgCtrlID(h):h for h in gui.windows(hwnd) if u.GetDlgCtrlID(h)>0}
            return proc,hwnd,controls
        def close(proc,hwnd):
            u.PostMessageW(hwnd,0x0010,0,0)
            try:
                proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                proc.kill();proc.wait(timeout=15)
                raise
            assert proc.returncode==0
        def config(mode):
            values={"DVDA_SRC":str(root/"audio"),"DVDA_BUILD_DIR":str(root/mode/"work"),
                "DVDA_FINAL_DIR":str(root/mode/"output"),"DVDA_MLP_SOURCE":mode,
                "DVDA_TITLE":"Test","DVDA_ISO_PREFIX":"disc","DVDA_MENU":"on",
                "DVDA_MENU_STILLPICS":"on","DVDA_MENU_TRACKS_PER_PAGE":"1","DVDA_TITLE_MODE":"1",
                "DVDA_MENU_INDEX_MIN_ALBUMS":"1"}
            profile.write_text(json.dumps({"Version":1,"Language":"en","Values":values}),encoding="utf-8")
        config("surcode-batch")
        proc,hwnd,controls=launch()
        runtime=next((root/"local/DVD-Audio-Maker/runtime").glob("*/dvda-media.dll")).parent
        # Only EXE was copied. Everything else used by the build came from it.
        subprocess.run([sys.executable,__file__,"--generate",str(runtime),str(root/"audio")],
            check=True,env=env,creationflags=subprocess.CREATE_NO_WINDOW)
        completed=[]
        timings=[]
        for mode in ["surcode-batch","lpcm"]:
            if mode=="lpcm":
                config(mode);proc,hwnd,controls=launch()
            try:
                for action in [106,107]:
                    started=time.monotonic()
                    print(f"{mode}: {'build' if action==106 else 'verify'}",flush=True)
                    # Queue commands like real input: synchronous cross-process
                    # SendMessage can reenter completion while it enables controls.
                    u.PostMessageW(hwnd,0x0111,action,controls[action])
                    gui.wait_for(lambda:not u.IsWindowEnabled(controls[106]),5)
                    gui.wait_for(lambda:u.IsWindowEnabled(controls[106]),180)
                    time.sleep(.1)
                    logs=gui.text(controls[115])
                    assert logs.rstrip().endswith("Task completed."),logs
                    timings.append({"mode":mode,"action":action,"seconds":round(time.monotonic()-started,3)})
                    print(json.dumps(timings[-1]),flush=True)
                assert (root/mode/"output/disc_1.iso").is_file()
                assert (root/mode/"work/build.log").is_file()
                assert (root/mode/"work/decode_report.txt").read_text(encoding="utf-8").startswith("Source audio verification report")
                completed.append(mode)
            except Exception:
                evidence=args.output.parent/(root.name+"-failed")
                shutil.copytree(root,evidence,ignore=shutil.ignore_patterns("local","DVD-Audio-Maker.exe"))
                (evidence/"gui.log").write_text(gui.text(controls[115]),encoding="utf-8")
                print(f"Failure evidence: {evidence}",flush=True)
                raise
            finally:
                close(proc,hwnd)
        encoded=runtime/"mlp_encoder.dll"
        expected=hashlib.sha256(encoded.read_bytes()).hexdigest()
        encoded.write_bytes(b"damaged runtime")
        proc,hwnd,_=launch()
        assert hashlib.sha256(encoded.read_bytes()).hexdigest()==expected
        close(proc,hwnd)
        record={"passed":True,"build_and_verify":completed,"menu_and_stills":True,
            "runtime_repair":True,"standalone_exe":True,"runtime_files":len(list(runtime.rglob("*"))),
            "executable_sha256":hashlib.sha256(exe.read_bytes()).hexdigest(),
            "source_frames":[192013,192020],"sample_rate":48000,"timings":timings}
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(record,indent=2)+"\n",encoding="utf-8")
        print(json.dumps(record))

if __name__=="__main__":main()
