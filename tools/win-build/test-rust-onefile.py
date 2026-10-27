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
    subprocess.run([str(fixture_ffmpeg),"-hide_banner","-nostdin","-v","error",
        "-f","lavfi","-i","testsrc2=size=320x320:rate=1","-frames:v","1",
        "-y",str(root/"cover.png")],check=True,creationflags=subprocess.CREATE_NO_WINDOW)

def mpeg_sequences(path):
    """(width, height, frame-rate code) of each 720-wide MPEG-2 sequence header in a disc image.

    Menu pages and playback stills are the only video on a DVD-Audio disc. The
    width filter drops start-code look-alikes inside audio payloads.
    """
    data=path.read_bytes()
    found,start=[],0
    while (start:=data.find(b"\x00\x00\x01\xb3",start))>=0:
        b=data[start+4:start+8]
        width,height=(b[0]<<4)|(b[1]>>4),((b[1]&0x0f)<<8)|b[2]
        if width==720:
            found.append((width,height,b[3]&0x0f))
        start+=4
    return found

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe",type=Path)
    parser.add_argument("--require-static", action="store_true")
    parser.add_argument("--generate",nargs=2,type=Path)
    parser.add_argument("--fixture-ffmpeg",type=Path,default=shutil.which("ffmpeg"),
        help="Independent FFmpeg executable used only to generate FLAC test inputs")
    parser.add_argument("--output",type=Path,default=Path("build/onefile-rust-validation.json"))
    parser.add_argument("--norm",choices=("pal","secam","ntsc"),default="pal",
        help="Menu video standard written to the profile and checked in the authored ISO")
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
        def remove_test_directory():
            deadline=time.monotonic()+15
            while root.exists():
                try:
                    shutil.rmtree(root)
                except PermissionError:
                    if time.monotonic()>=deadline:
                        raise
                    time.sleep(.1)
        cleanup.callback(remove_test_directory)
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
            gui.wait_for(lambda:not runtime.exists(),10)
        def config(mode):
            values={"DVDA_SRC":str(root/"audio"),"DVDA_BUILD_DIR":str(root/mode/"work"),
                "DVDA_FINAL_DIR":str(root/mode/"output"),"DVDA_MLP_SOURCE":mode,
                "DVDA_TITLE":"Test","DVDA_ISO_PREFIX":"disc","DVDA_MENU":"on",
                "DVDA_MENU_STILLPICS":"on","DVDA_MENU_TRACKS_PER_PAGE":"1","DVDA_TITLE_MODE":"1",
                "DVDA_MENU_INDEX_MIN_ALBUMS":"1","DVDA_MENU_NORM":args.norm}
            profile.write_text(json.dumps({"Version":1,"Language":"en","Values":values}),encoding="utf-8")
        config("mlp-batch")
        proc,hwnd,controls=launch()
        runtime=next((Path(tempfile.gettempdir())/"DVD-Audio-Maker-tasks").glob(f"task-{proc.pid}-*/runtime/*/type.xml")).parent
        runtime_paths=[p for p in runtime.rglob("*") if p.is_file()]
        runtime_dlls=[p for p in runtime_paths if p.suffix.casefold()==".dll"]
        if args.require_static:
            assert not runtime_dlls, runtime_dlls
            sys.path.insert(0, str(Path(__file__).parent / "native"))
            from pe_dependencies import Pe
            imports = Pe(exe).imports()
            assert not any(name.startswith(("avcodec-", "avformat-", "avutil-", "swscale-", "swresample-")) for name in imports), imports
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
        for mode in ["mlp-batch","lpcm"]:
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
                    page=next(h for h in gui.windows(hwnd)
                        if gui.classname(h)=="DVD_AUDIO_PAGE_RUST" and u.IsWindowVisible(h))
                    assert u.IsWindowEnabled(page), "busy settings page must accept scrolling"
                    assert not u.IsWindowEnabled(controls[200]), "busy source field must be read-only"
                    u.SetWindowPos(hwnd,None,40,40,800,800,0x0040)
                    scroll_checks=0
                    deadline=time.monotonic()+180
                    while not u.IsWindowEnabled(controls[106]):
                        assert time.monotonic()<deadline, "task timed out"
                        u.SendMessageW(page,0x0115,7,0)
                        bottom=u.GetWindowLongPtrW(page,-21)
                        u.SendMessageW(page,0x0115,6,0)
                        top=u.GetWindowLongPtrW(page,-21)
                        assert bottom>top and top==0, (bottom,top)
                        scroll_checks+=1
                        time.sleep(.02)
                    assert u.IsWindowEnabled(controls[200]), "source field did not recover"
                    print(f"busy scroll cycles: {scroll_checks}",flush=True)
                    time.sleep(.1)
                    logs=gui.text(controls[115])
                    latest_log["text"]=logs
                    assert logs.rstrip().endswith("Task completed."),logs
                    timings.append({"mode":mode,"action":action,"seconds":round(time.monotonic()-started,3)})
                    print(json.dumps(timings[-1]),flush=True)
                assert (root/mode/"output/disc_1.iso").is_file()
                # NTSC menus are 720x480 at 29.97 fps (code 4); PAL and SECAM 720x576 at 25 fps (code 3).
                expected=(720,480,4) if args.norm=="ntsc" else (720,576,3)
                sequences=mpeg_sequences(root/mode/"output/disc_1.iso")
                assert sequences and set(sequences)=={expected}, (args.norm,sorted(set(sequences)))
                print(f"{mode}: {len(sequences)} menu/still sequence headers, all {expected}",flush=True)
                assert not (root/mode/"work/unfinished-task").exists(), "successful task workspace was not cleaned"
                mode_checks.append({"mode":mode,"iso_created":True,"source_verification_passed":True})
                completed.append(mode)
            except Exception:
                latest_log["text"]=gui.text(controls[115])
                raise
            finally:
                close(proc,hwnd)
        record={"passed":True,"build_and_verify":completed,"menu_and_stills":True,"menu_norm":args.norm,
            "standalone_exe":True,"runtime_files":len(runtime_paths),
            "mode_checks":mode_checks,
            "project_owned_runtime_dlls":[],"runtime_dlls":[p.name for p in runtime_dlls],"author_subprocess_shipped":False,
            "system_only_path":True,"fixture_generator":str(fixture_ffmpeg),
            "executable_sha256":hashlib.sha256(exe.read_bytes()).hexdigest(),
            "source_frames":[192013,192020],"sample_rate":48000,"timings":timings}
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(record,indent=2)+"\n",encoding="utf-8")
        print(json.dumps(record))

if __name__=="__main__":main()
