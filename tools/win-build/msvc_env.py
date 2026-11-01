"""Find Visual Studio and return an initialized native Windows x64/ARM64 environment."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess


def find_vsdevcmd(base_environment=None, arch='x64'):
    env = dict(os.environ if base_environment is None else base_environment)
    override = env.get('DVDA_VSDEVCMD')
    if override:
        path = Path(override).expanduser().resolve()
        if not path.is_file():
            raise FileNotFoundError(f'DVDA_VSDEVCMD does not name VsDevCmd.bat: {path}')
        return path
    vswhere = shutil.which('vswhere.exe', path=env.get('PATH'))
    if not vswhere:
        installer = Path(env.get('ProgramFiles(x86)', r'C:\Program Files (x86)'))
        candidate = installer/'Microsoft Visual Studio/Installer/vswhere.exe'
        if candidate.is_file():
            vswhere = str(candidate)
    if vswhere:
        found = subprocess.run(
            [vswhere, '-latest', '-products', '*', '-requires',
             'Microsoft.VisualStudio.Component.VC.Tools.ARM64'
             if arch == 'arm64'
             else 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
             '-property', 'installationPath'],
            env=env, capture_output=True, text=True, check=True,
        ).stdout.strip()
        if found:
            path = Path(found)/'Common7/Tools/VsDevCmd.bat'
            if path.is_file():
                return path.resolve()
    if env.get('VSINSTALLDIR'):
        path = Path(env['VSINSTALLDIR'])/'Common7/Tools/VsDevCmd.bat'
        if path.is_file():
            return path.resolve()
    raise FileNotFoundError(
        'MSVC builds require Visual Studio C++ x64 tools. Install the C++ build '
        'tools or set DVDA_VSDEVCMD to the installed VsDevCmd.bat.'
    )


def msvc_environment(base_environment=None, arch='x64'):
    env = dict(os.environ if base_environment is None else base_environment)
    # Keep an already initialized x64 native-tools environment, while checking
    # the compiler and librarian actually resolve in its PATH.
    if env.get('VSCMD_ARG_TGT_ARCH', '').lower() == arch and all(
        shutil.which(name, path=env.get('PATH')) for name in ('cl.exe', 'lib.exe')
    ):
        result = env
    else:
        vsdevcmd = find_vsdevcmd(env, arch)
        command = f'call "{vsdevcmd}" -no_logo -arch={arch} -host_arch=x64 >nul && set'
        # cmd.exe uses doubled outer quotes, unlike the C argv parser used by
        # subprocess.list2cmdline; preserve the batch path's literal quotes.
        cmdline = subprocess.list2cmdline([env.get('COMSPEC', 'cmd.exe')])
        cmdline += ' /d /s /c "'+command+'"'
        process = subprocess.run(
            cmdline,
            env=env, capture_output=True, text=True,
        )
        if process.returncode:
            raise RuntimeError(
                f'Visual Studio {arch} environment initialization failed: {vsdevcmd}\n'
                f'{process.stderr.strip() or process.stdout.strip()}'
            )
        result = dict(env)
        for line in process.stdout.splitlines():
            key, separator, value = line.partition('=')
            if separator and key:
                result[key] = value
    for name in ('cl.exe', 'lib.exe', 'link.exe', 'dumpbin.exe'):
        if not shutil.which(name, path=result.get('PATH')):
            raise FileNotFoundError(f'MSVC {arch} environment does not provide {name}')
    result['CC'] = shutil.which('cl.exe', path=result.get('PATH'))
    result['DVDA_LIB_TOOL'] = shutil.which('lib.exe', path=result.get('PATH'))
    return result


def is_vc_runtime(name):
    name = Path(name).name.lower()
    return name.endswith('.dll') and name.startswith(('vcruntime140', 'msvcp140', 'concrt140'))


def vc_runtime_roots(environment=None):
    env = os.environ if environment is None else environment
    redist = []
    if env.get('VCToolsRedistDir'):
        redist.append(Path(env['VCToolsRedistDir']))
    if env.get('VSINSTALLDIR'):
        redist.extend(sorted((Path(env['VSINSTALLDIR'])/'VC/Redist/MSVC').glob('*'), reverse=True))
    roots = []
    for directory in redist:
        for path in sorted((directory/'x64').glob('Microsoft.VC*.CRT'), reverse=True):
            if path.is_dir() and path not in roots:
                roots.append(path)
    roots.append(Path(env.get('SystemRoot', r'C:\Windows'))/'System32')
    return roots


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    output = parser.add_mutually_exclusive_group()
    parser.add_argument('--arch', choices=('x64', 'arm64'), default='x64',
                        help='Target architecture of the initialized MSVC environment (default x64)')
    output.add_argument('--json', action='store_true', help='Print environment as JSON for captured build-script import')
    output.add_argument('--runtime-roots-json', action='store_true', help='Print VC redistributable search directories as JSON')
    args = parser.parse_args()
    environment = msvc_environment(arch=args.arch)
    if args.json:
        print(json.dumps(environment))
    elif args.runtime_roots_json:
        print(json.dumps([str(path) for path in vc_runtime_roots(environment)]))
    else:
        print(f'MSVC {args.arch} compiler: '+environment['CC'])
        print(f'MSVC {args.arch} librarian: '+environment['DVDA_LIB_TOOL'])
