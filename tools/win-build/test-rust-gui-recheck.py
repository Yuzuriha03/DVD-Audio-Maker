"""Regression for GUI review R06-R09; all profiles and audio live in temporary folders."""
import argparse
from contextlib import contextmanager
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import time
import wave

u = c.WinDLL("user32", use_last_error=True)
CALLBACK = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
u.EnumWindows.argtypes = [CALLBACK, w.LPARAM]
u.EnumChildWindows.argtypes = [w.HWND, CALLBACK, w.LPARAM]
u.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
u.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
u.GetDlgCtrlID.argtypes = [w.HWND]
u.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.SendMessageW.restype = w.LPARAM
u.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
u.IsWindowEnabled.argtypes = [w.HWND]


def windows(parent=None):
    found = []
    callback = CALLBACK(lambda hwnd, _: found.append(hwnd) or True)
    if parent:
        u.EnumChildWindows(parent, callback, 0)
    else:
        u.EnumWindows(callback, 0)
    return found


def class_name(hwnd):
    buffer = c.create_unicode_buffer(256)
    u.GetClassNameW(hwnd, buffer, len(buffer))
    return buffer.value


def text(hwnd):
    buffer = c.create_unicode_buffer(u.SendMessageW(hwnd, 0x000E, 0, 0) + 1)
    u.SendMessageW(hwnd, 0x000D, len(buffer), c.addressof(buffer))
    return buffer.value


def set_text(hwnd, value):
    buffer = c.create_unicode_buffer(value)
    u.SendMessageW(hwnd, 0x000C, 0, c.addressof(buffer))


def wait_for(condition, seconds=30):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = condition()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError("GUI condition timed out")


def owned_windows(pid):
    result = []
    for hwnd in windows():
        owner = w.DWORD()
        u.GetWindowThreadProcessId(hwnd, c.byref(owner))
        if owner.value == pid:
            result.append(hwnd)
    return result


def find_window(pid, kind):
    return next((h for h in owned_windows(pid) if class_name(h) == kind), None)


def write_profile(path, values, language="en"):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({"Version": 1, "Language": language, "Values": values},
        ensure_ascii=False), encoding="utf-8")


def read_profile(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def command(hwnd, controls, ident):
    u.SendMessageW(hwnd, 0x0111, ident, controls[ident])


def select(hwnd, controls, ident, index):
    u.SendMessageW(controls[ident], 0x014E, index, 0)
    u.SendMessageW(hwnd, 0x0111, ident | (1 << 16), controls[ident])


@contextmanager
def launch(exe, args, root, runtime=None, environment_language=None, overrides=None):
    root.mkdir(parents=True, exist_ok=True)
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith("DVDA_")}
    env["LOCALAPPDATA"] = str(root / "local")
    if runtime:
        env["DVDA_MEDIA_NATIVE_DIR"] = str(runtime)
    if environment_language is not None:
        env["DVDA_LANGUAGE"] = environment_language
    env.update(overrides or {})
    proc = subprocess.Popen([str(exe), *map(str, args)], cwd=root, env=env,
        creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        yield proc
    finally:
        if proc.poll() is None:
            for hwnd in owned_windows(proc.pid):
                u.PostMessageW(hwnd, 0x0010, 0, 0)
            try:
                proc.wait(timeout=15)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=5)
                raise AssertionError("GUI did not close after test")
        assert not owned_windows(proc.pid), "Test GUI windows survived cleanup"


def ready(proc, title):
    hwnd = wait_for(lambda: find_window(proc.pid, "DVD_AUDIO_MAKER_RUST"))
    def loaded():
        controls = {u.GetDlgCtrlID(h): h for h in windows(hwnd) if u.GetDlgCtrlID(h) > 0}
        required = {102, 103, 105, 106, 107, 110, 111, 115, 202, 205, 206, 301, 302, 305}
        return controls if required.issubset(controls) and text(controls[202]) == title else None
    return hwnd, wait_for(loaded)


def generate_flacs(runtime, folder):
    class Request(c.Structure):
        _fields_ = [(n, c.c_uint32) for n in
            ("size", "abi", "operation", "rate", "bits", "format", "soxr", "compression", "cover", "tag_count")] + [
            ("input", c.c_char_p), ("output", c.c_char_p), ("tags", c.POINTER(c.c_char_p))]
    emit_type = c.CFUNCTYPE(None, c.c_void_p, c.c_int, c.c_char_p)
    cancel_type = c.CFUNCTYPE(c.c_int, c.c_void_p)
    library = c.CDLL(str(runtime / "dvda-media.dll"))
    library.dvdamedia_run.argtypes = [c.POINTER(Request), emit_type, cancel_type, c.c_void_p]
    folder.mkdir(parents=True)
    logs = []
    emit = emit_type(lambda _, stream, message: logs.append(message.decode("utf-8", "replace")))
    cancel = cancel_type(lambda _: 0)
    for channels in [1, 2]:
        source = folder / f"track-{channels}.wav"
        with wave.open(str(source), "wb") as writer:
            writer.setparams((channels, 2, 48000, 0, "NONE", "not compressed"))
            writer.writeframes(b"".join(struct.pack("<h", ((frame * 97 + channel * 113) % 60001) - 30000)
                for frame in range(4800) for channel in range(channels)))
        tags = (c.c_char_p * 6)(b"ALBUM", b"Channel audit", b"TITLE", f"Track {channels}".encode(),
            b"TRACK", str(channels).encode())
        request = Request(c.sizeof(Request), 1, 3, 0, 0, 6, 0, 8, 0, 3,
            str(source).encode("utf-8"), str(source.with_suffix(".flac")).encode("utf-8"), tags)
        assert library.dvdamedia_run(c.byref(request), emit, cancel, None) == 0, logs
        source.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path)
    parser.add_argument("--runtime", type=Path, help="Development media directory; omit for the bundled EXE")
    parser.add_argument("--generate", nargs=2, type=Path)
    parser.add_argument("--output", type=Path, default=Path("build/gui-recheck-validation.json"))
    args = parser.parse_args()
    if args.generate:
        generate_flacs(*args.generate)
        return
    if not args.exe:
        parser.error("--exe is required")
    exe = args.exe.resolve()
    runtime = args.runtime.resolve() if args.runtime else None
    records = []
    with tempfile.TemporaryDirectory(prefix="dvda-gui-recheck-") as temporary:
        root = Path(temporary)
        case = root / "derived"
        profile = case / "profile.json"
        write_profile(profile, {"DVDA_TITLE": "Original title", "DVDA_ISO_PREFIX": "",
            "DVDA_BUILD_DIR": str(case / "work"), "DVDA_MLP_EXTERNAL_DIR": ""})
        with launch(exe, ["--profile", profile], case, runtime) as proc:
            hwnd, controls = ready(proc, "Original title")
            assert text(controls[206]) == text(controls[305]) == "", "Automatic values became fixed values"
            set_text(controls[202], "New title")
            set_text(controls[205], str(case / "changed-work"))
            command(hwnd, controls, 102)
            values = read_profile(profile)["Values"]
            assert values["DVDA_ISO_PREFIX"] == values["DVDA_MLP_EXTERNAL_DIR"] == "", values
            assert values["DVDA_TITLE"] == "New title"
            assert values["DVDA_BUILD_DIR"] == str(case / "changed-work")
            records.append({"case": "derived_values_remain_automatic", "saved": values})
            set_text(controls[206], "explicit-prefix")
            set_text(controls[305], str(case / "explicit-mlp"))
            command(hwnd, controls, 102)
        with launch(exe, ["--profile", profile], case, runtime) as proc:
            _, controls = ready(proc, "New title")
            assert text(controls[206]) == "explicit-prefix"
            assert text(controls[305]) == str(case / "explicit-mlp")
            records.append({"case": "explicit_derived_overrides_survive_reload", "passed": True})
        explicit = {"DVDA_ISO_PREFIX": "environment-prefix", "DVDA_MLP_EXTERNAL_DIR": str(case / "environment-mlp")}
        with launch(exe, ["--profile", profile], case, runtime, overrides=explicit) as proc:
            hwnd, controls = ready(proc, "New title")
            assert text(controls[206]) == explicit["DVDA_ISO_PREFIX"]
            assert text(controls[305]) == explicit["DVDA_MLP_EXTERNAL_DIR"]
            command(hwnd, controls, 102)
            values = read_profile(profile)["Values"]
            assert all(values[key] == value for key, value in explicit.items())
            records.append({"case": "explicit_environment_overrides_preserved", "saved": explicit})

        for name, flags, env_language, expected in [
            ("config_alias", ["--config", "PROFILE"], None, 2),
            ("profile_alias", ["--profile", "PROFILE"], None, 2),
            ("language_cli", ["--profile", "PROFILE", "--language", "en"], None, 1),
            ("language_cli_first", ["--language", "zh-CN", "--config", "PROFILE"], "en", 0),
            ("language_environment", ["--profile", "PROFILE"], "en", 1),
            ("language_cli_wins", ["--config", "PROFILE", "--language", "en"], "ja", 1),
        ]:
            case = root / name
            profile = case / "chosen 日本語.json"
            write_profile(profile, {"DVDA_TITLE": "Chosen marker"}, "ja")
            write_profile(case / "local/DVD-Audio-Maker/settings.json", {"DVDA_TITLE": "Daily marker"})
            flags = [str(profile) if value == "PROFILE" else value for value in flags]
            with launch(exe, flags, case, runtime, env_language) as proc:
                _, controls = ready(proc, "Chosen marker")
                actual = u.SendMessageW(controls[103], 0x0147, 0, 0)
                assert actual == expected, (name, actual, expected)
                records.append({"case": name, "title": text(controls[202]), "language_index": actual})

        for index, flags in enumerate([["--unknown"], ["--config"], ["--profile"],
            ["--language"], ["--language", "bad"], ["--language", "en", "--language", "ja"]]):
            case = root / f"invalid-args-{index}"
            with launch(exe, flags, case, runtime) as proc:
                dialog = wait_for(lambda: find_window(proc.pid, "#32770"))
                error = wait_for(lambda: "\n".join(text(h) for h in windows(dialog) if class_name(h) == "Static").strip())
                assert "Usage:" in error or all(code in error for code in ["auto", "en", "zh-CN", "ja"]), error
                assert not find_window(proc.pid, "DVD_AUDIO_MAKER_RUST")
                wait_for(lambda: any(class_name(h) == "Button" for h in windows(dialog)))
                u.PostMessageW(dialog, 0x0010, 0, 0)
                proc.wait(timeout=10)
                assert proc.returncode == 1, proc.returncode
                records.append({"case": "invalid_startup_arguments", "arguments": flags, "error": error})

        case = root / "invalid-audio"
        profile = case / "profile.json"
        daily = case / "local/DVD-Audio-Maker/settings.json"
        write_profile(profile, {"DVDA_TITLE": "Invalid audio", "DVDA_MLP_SURCODE_SAMPLE_RATE": "12345",
            "DVDA_MLP_SURCODE_BITS": "17"})
        write_profile(daily, {"DVDA_TITLE": "Untouched daily"})
        with launch(exe, ["--profile", profile], case, runtime) as proc:
            hwnd, controls = ready(proc, "Invalid audio")
            assert text(controls[301]) == "12345" and text(controls[302]) == "17"
            command(hwnd, controls, 102)
            saved = read_profile(profile)["Values"]
            assert saved["DVDA_MLP_SURCODE_SAMPLE_RATE"] == "12345"
            assert saved["DVDA_MLP_SURCODE_BITS"] == "17"
            before, daily_before = profile.read_bytes(), daily.read_bytes()
            for action in [105, 106, 107]:
                command(hwnd, controls, action)
                assert profile.read_bytes() == before
                assert daily.read_bytes() == daily_before
                assert u.IsWindowEnabled(controls[106]), "Invalid configuration started a task"
                wait_for(lambda: "Invalid target sample rate for MLP." in text(controls[115]))
            select(hwnd, controls, 103, 2)
            assert text(controls[301]) == "12345" and text(controls[302]) == "17"
        assert profile.read_bytes() == before
        saved_daily = read_profile(daily)["Values"]
        assert saved_daily["DVDA_MLP_SURCODE_SAMPLE_RATE"] == "12345"
        assert saved_daily["DVDA_MLP_SURCODE_BITS"] == "17"
        records.append({"case": "invalid_audio_preserved_and_actions_rejected", "rate": "12345", "bits": "17",
            "profile_sha256": hashlib.sha256(before).hexdigest(), "daily_preserves_unknown_values_after_close": True})
        with launch(exe, ["--profile", profile], case, runtime) as proc:
            hwnd, controls = ready(proc, "Invalid audio")
            select(hwnd, controls, 301, 1)
            command(hwnd, controls, 105)
            assert profile.read_bytes() == before
            wait_for(lambda: "16, 20 or 24" in text(controls[115]))
            select(hwnd, controls, 302, 2)
            command(hwnd, controls, 102)
            saved = read_profile(profile)["Values"]
            assert saved["DVDA_MLP_SURCODE_SAMPLE_RATE"] == "48000"
            assert saved["DVDA_MLP_SURCODE_BITS"] == "24"
            records.append({"case": "audio_settings_require_explicit_correction", "rate": "48000", "bits": "24"})

        case = root / "prepare"
        profile = case / "profile.json"
        source = case / "audio 日本語"
        write_profile(profile, {"DVDA_TITLE": "Preparation issues", "DVDA_SRC": str(source),
            "DVDA_BUILD_DIR": str(case / "work"), "DVDA_MENU": "off"})
        with launch(exe, ["--profile", profile], case, runtime) as proc:
            hwnd, controls = ready(proc, "Preparation issues")
            fixture_runtime = runtime or next((case / "local/DVD-Audio-Maker/runtime").glob("*/dvda-media.dll")).parent
            subprocess.run([sys.executable, str(Path(__file__).resolve()), "--generate", str(fixture_runtime), str(source)],
                check=True, creationflags=subprocess.CREATE_NO_WINDOW)
            command(hwnd, controls, 105)
            wait_for(lambda: u.IsWindowEnabled(controls[106]) and "Task failed." in text(controls[115]))
            summary = text(controls[115])
            assert "[FAIL]" in summary and "48000" in summary and str(source) in summary, summary
            select(hwnd, controls, 110, 1)
            detailed = text(controls[115])
            assert "[FAIL]" in detailed and "48000" in detailed and str(source) in detailed, detailed
            select(hwnd, controls, 110, 0)
            u.SendMessageW(controls[111], 0x00F1, 1, 0)
            command(hwnd, controls, 111)
            problems = text(controls[115])
            assert "[FAIL]" in problems and str(source) in problems, problems
            report = (case / "work/decode_report.txt").read_text(encoding="utf-8-sig")
            assert "[FAIL]" in report
            languages = {}
            for language, index, expected in [
                ("zh-CN", 0, "声道数不一致"),
                ("en", 1, "channel"),
                ("ja", 2, "チャンネル数が一致しません"),
            ]:
                select(hwnd, controls, 103, index)
                displayed = text(controls[115])
                assert "[FAIL]" in displayed and str(source) in displayed, displayed
                assert expected in displayed, displayed
                channel_counts = {
                    "zh-CN": "1 声道 × 1 首; 2 声道 × 1 首",
                    "en": "1 channels × 1 tracks; 2 channels × 1 tracks",
                    "ja": "1 チャンネル × 1 曲; 2 チャンネル × 1 曲",
                }
                assert channel_counts[language] in displayed, displayed
                languages[language] = displayed
        archives = "\n".join(p.read_text(encoding="utf-8-sig") for p in (case / "local/DVD-Audio-Maker/logs").glob("*"))
        assert "[FAIL]" in archives and str(source) in archives, archives
        records.append({"case": "prepare_channel_issue_visible_and_archived", "summary": summary,
            "detailed": detailed, "problems": problems, "languages": languages,
            "report": report, "archive_contains_issue": True})

    result = {"passed": True, "executable": str(exe), "executable_sha256": hashlib.sha256(exe.read_bytes()).hexdigest(),
        "user_profile_isolated": True, "all_test_processes_closed": True, "cases": records}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": True, "cases": len(records), "output": str(args.output)}))


if __name__ == "__main__":
    main()
