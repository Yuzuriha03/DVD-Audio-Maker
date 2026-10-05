"""Check the actual developer batch entrypoint with an isolated native cargo fixture."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[2]


def main():
    with tempfile.TemporaryDirectory(prefix="dvda-buildcmd-") as temporary:
        root = Path(temporary)
        source = root / "fixture.c"
        source.write_text(r'''
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
int main(int argc, char **argv) {
    FILE *log = fopen(getenv("DVDA_SCRIPT_CALL_LOG"), "ab");
    if (!log) return 12;
    int prepare = 0;
    for (int i=1; i<argc; ++i) {
        fprintf(log, "%s%s", i==1 ? "" : "|", argv[i]);
        if (!strcmp(argv[i], "prepare")) prepare=1;
    }
    fputc('\n',log); fclose(log);
    return prepare ? atoi(getenv("DVDA_SCRIPT_PREPARE_STATUS")) : 0;
}
''', encoding="ascii")
        subprocess.run(["gcc", "-std=c17", "-O2", "-static", str(source), "-o", str(root / "cargo.exe")], check=True, capture_output=True)
        profile = root / "profile with spaces.json"
        profile.write_text('{"Version":1,"Values":{}}', encoding="utf-8")
        scenarios = [([], 0), (["--dry-run"], 0), (["--profile", str(profile)], 0),
                     (["--dry-run", "--profile", str(profile)], 0),
                     (["--profile", str(profile), "--dry-run"], 0), ([], 3)]
        results = []
        for arguments, status in scenarios:
            log = root / "calls.txt"
            log.unlink(missing_ok=True)
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"],
                       DVDA_SCRIPT_CALL_LOG=str(log), DVDA_SCRIPT_PREPARE_STATUS=str(status))
            # The real script is called unchanged; cmd never deletes/moves paths.
            command = subprocess.list2cmdline([str(REPO / "build.cmd"), *arguments])
            command_line = subprocess.list2cmdline([os.environ.get("COMSPEC", "cmd.exe")]) + ' /d /s /c "' + command + '"'
            result = subprocess.run(command_line,
                                    cwd=root, env=env, capture_output=True)
            calls = log.read_text().splitlines() if log.exists() else []
            assert result.returncode == (1 if status else 0), (arguments, result.returncode, result.stdout, result.stderr)
            assert len(calls) == (1 if status else 2), calls
            assert "|prepare" in calls[0], calls
            assert all("|--target|x86_64-pc-windows-gnu|" in call for call in calls)
            if not status:
                assert "|build" in calls[1], calls
                assert ("|--dry-run" in calls[1]) == ("--dry-run" in arguments)
                assert "|--dry-run" not in calls[0]
            if "--profile" in arguments:
                assert all("|--profile|" + str(profile) in call for call in calls)
            results.append({"arguments": arguments, "prepare_status": status, "exit_code": result.returncode, "calls": calls})
        for script, arguments, expected in [
            ("cli.cmd", [], "|config"),
            ("cli.cmd", ["config", "--config", str(profile)], "|config|--config|" + str(profile)),
            ("verify.cmd", ["capacity", "--profile", str(profile)], "|verify|capacity|--profile|" + str(profile)),
        ]:
            log.unlink(missing_ok=True)
            env["DVDA_SCRIPT_PREPARE_STATUS"] = "0"
            command = subprocess.list2cmdline([str(REPO / script), *arguments])
            command_line = subprocess.list2cmdline([os.environ.get("COMSPEC", "cmd.exe")]) + ' /d /s /c "' + command + '"'
            result = subprocess.run(command_line, cwd=root, env=env, capture_output=True)
            calls = log.read_text().splitlines() if log.exists() else []
            assert result.returncode == 0 and len(calls) == 1, (script, result, calls)
            assert expected in calls[0] and "|--target|x86_64-pc-windows-gnu|" in calls[0], calls
            if "--config" in arguments:
                assert "|--profile|" not in calls[0], calls
            results.append({"script":script, "arguments":arguments, "exit_code":result.returncode,"calls":calls})
        output = REPO / "build/rust-build-entrypoints-validation.json"
        output.write_text(json.dumps({"passed": len(results), "cases": results}, indent=2), encoding="utf-8")
        print(f"PASS: {len(results)} real build.cmd branch/failure cases; {output}")


if __name__ == "__main__":
    main()
