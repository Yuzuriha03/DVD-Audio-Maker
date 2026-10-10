"""Run the shipped EXE with generated audio, no development DLL paths or user profile."""
import argparse, contextlib, hashlib, importlib.util, json, os, shutil, struct, subprocess, sys, tempfile, time, wave
from pathlib import Path

def generate(fixture_ffmpeg, root):
    # This independent tool creates test inputs only. The GUI receives an
    # environment containing only the shipped runtime and Windows libraries.
    root.mkdir()
    for n in range(2):
        input=root/f"{n}.wav"
        with wave.open(str(input),"wb") as output:
            output.setparams((2,2,48000,0,"NONE","not compressed"))
            output.writeframes(b"".join(struct.pack("<h",((f*97+ch*113+n*719)%60001)-30000)
                # Keep enough PTS steps for one title reset to stay within the
                # legacy 1% abnormal-step limit; retain a partial final MLP AU.
                for f in range(192013+n*7) for ch in range(2)))
        subprocess.run([str(fixture_ffmpeg),"-hide_banner","-nostdin","-v","error",
            "-i",str(input),"-c:a","flac","-compression_level","8",
            "-metadata","ALBUM=Test album","-metadata",f"TITLE=Track {n}",
            "-metadata",f"TRACK={n+1}","-metadata","DATE=2026",str(root/f"{n}.flac")],
            check=True,creationflags=subprocess.CREATE_NO_WINDOW)
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
    parser.add_argument("--fixture-ffmpeg",type=Path,default=shutil.which("ffmpeg"),
        help="Independent FFmpeg executable used only to generate FLAC test inputs")
    parser.add_argument("--output",type=Path,default=Path("build/onefile-rust-validation.json"))
    args=parser.parse_args()
    if args.generate:
        generate(*args.generate)
        return
    if not args.exe or not args.fixture_ffmpeg or not args.fixture_ffmpeg.is_file():
        parser.error("--exe and an available --fixture-ffmpeg are required")
    fixture_ffmpeg=args.fixture_ffmpeg.resolve()
    args.output.parent.mkdir(parents=True,exist_ok=True)
    spec=importlib.util.spec_from_file_location("gui",Path(__file__).with_name("test-rust-gui.py"))
    gui=importlib.util.module_from_spec(spec);spec.loader.exec_module(gui)
    u=gui.u
    with tempfile.TemporaryDirectory(prefix="dvda-onefile-") as temporary, contextlib.ExitStack() as cleanup:
        root=Path(temporary)
        latest_log={"text":""}
        def failure_evidence(exc_type,exc,traceback):
            if exc_type is not None:
                evidence=args.output.parent/(root.name+"-failed")
                shutil.copytree(root,evidence,dirs_exist_ok=True,
                    ignore=shutil.ignore_patterns("local","DVD-Audio-Maker.exe"))
                (evidence/"error.txt").write_text(str(exc),encoding="utf-8")
                if not (evidence/"gui.log").exists():
                    (evidence/"gui.log").write_text(latest_log["text"],encoding="utf-8")
                print(f"Failure evidence: {evidence}",flush=True)
            return False
        cleanup.push(failure_evidence)
        exe=root/"DVD-Audio-Maker.exe";shutil.copy2(args.exe,exe)
        env={k:v for k,v in os.environ.items() if not k.startswith(("DVDA_","MAGICK_"))}
        env["LOCALAPPDATA"]=str(root/"local")
        system=Path(os.environ.get("SystemRoot",r"C:\Windows"))
        env["PATH"]=str(system/"System32")+os.pathsep+str(system)
        profile=root/"settings.json"
        def stop(proc):
            if proc.poll() is None:
                proc.kill()
                proc.wait(timeout=15)
        def launch():
            proc=subprocess.Popen([str(exe),"--profile",str(profile)],cwd=root,env=env,
                creationflags=subprocess.CREATE_NO_WINDOW)
            cleanup.callback(stop,proc)
            try:
                hwnd=gui.wait_for(lambda:gui.find_app(proc.pid),30)
                gui.wait_for(lambda:any(u.GetDlgCtrlID(h)==115 for h in gui.windows(hwnd)))
                gui.wait_for(lambda:any(u.GetDlgCtrlID(h)==200 and gui.text(h)==str(root/"audio") for h in gui.windows(hwnd)))
                controls={u.GetDlgCtrlID(h):h for h in gui.windows(hwnd) if u.GetDlgCtrlID(h)>0}
            except Exception:
                stop(proc)
                raise
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
        runtime=next((root/"local/DVD-Audio-Maker/runtime").glob("*/avutil-61.dll")).parent
        runtime_paths=[p for p in runtime.rglob("*") if p.is_file()]
        runtime_dlls=[p for p in runtime_paths if p.suffix.casefold()==".dll"]
        assert not any(p.name.casefold().startswith("dvda-") or p.name.casefold()=="mlp_encoder.dll"
            for p in runtime_dlls),runtime_dlls
        assert not any(p.suffix.casefold()==".exe" for p in runtime_paths),runtime_paths
        # Only the GUI EXE was copied. Input generation is outside the product;
        # every library, font and menu resource used by the build came from it.
        subprocess.run([sys.executable,__file__,"--generate",str(fixture_ffmpeg),str(root/"audio")],
            check=True,env=env,creationflags=subprocess.CREATE_NO_WINDOW)
        completed=[]
        timings=[]
        mode_checks=[]
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
                    latest_log["text"]=logs
                    assert logs.rstrip().endswith("Task completed."),logs
                    timings.append({"mode":mode,"action":action,"seconds":round(time.monotonic()-started,3)})
                    print(json.dumps(timings[-1]),flush=True)
                assert (root/mode/"output/disc_1.iso").is_file()
                assert (root/mode/"work/build.log").is_file()
                assert (root/mode/"work/decode_report.txt").read_text(encoding="utf-8").startswith("Source audio verification report")
                index=json.loads((root/mode/"work/mlp_index.json").read_text(encoding="utf-8-sig"))
                tracks={path:value for path,value in index.items() if not path.startswith("__")}
                assert len(tracks)==2,tracks
                assert index["__meta__"]["mlp_source"]==mode,index["__meta__"]
                for path,track in tracks.items():
                    assert track["mlp_source"]==mode,track
                    with Path(path).open("rb") as stream:
                        header=stream.read(12)
                    if mode=="surcode-batch":
                        assert Path(path).suffix.casefold()==".mlp" and header[4:8]==b"\xf8\x72\x6f\xbb",(path,header)
                    else:
                        assert header[:4] in (b"RIFF",b"RF64") and header[8:12]==b"WAVE",(path,header)
                mode_checks.append({"mode":mode,"tracks":len(tracks),"input_codec_verified":True})
                completed.append(mode)
            except Exception:
                latest_log["text"]=gui.text(controls[115])
                raise
            finally:
                close(proc,hwnd)
        encoded=runtime/"avutil-61.dll"
        expected=hashlib.sha256(encoded.read_bytes()).hexdigest()
        encoded.write_bytes(b"damaged runtime")
        proc,hwnd,_=launch()
        try:
            assert hashlib.sha256(encoded.read_bytes()).hexdigest()==expected
        finally:
            close(proc,hwnd)
        record={"passed":True,"build_and_verify":completed,"menu_and_stills":True,
            "runtime_repair":True,"standalone_exe":True,"runtime_files":len(runtime_paths),
            "mode_checks":mode_checks,
            "project_owned_runtime_dlls":[],"author_subprocess_shipped":False,
            "system_only_path":True,"fixture_generator":str(fixture_ffmpeg),
            "executable_sha256":hashlib.sha256(exe.read_bytes()).hexdigest(),
            "source_frames":[192013,192020],"sample_rate":48000,"timings":timings}
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(record,indent=2)+"\n",encoding="utf-8")
        print(json.dumps(record))

if __name__=="__main__":main()
