"""Build pinned ImageMagick 7 Q16 HDRI and delegates with native MSVC x64 tools.

The upstream project generator is compiled without its optional MFC GUI. No
MinGW compiler, Unix shell, prebuilt codec library or UCRT jump shim is used.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import urllib.request
from zipfile import ZipFile

from msvc_env import msvc_environment
from worker_policy import worker_count

TARGET = 'x86_64-pc-windows-msvc'
VERSION = '7.1.2-33'
CODERS = ['JPEG', 'PNG', 'WEBP', 'XC', 'GRADIENT', 'INFO', 'NULL',
          'RGB', 'GRAY', 'LABEL', 'CAPTION', 'MPR', 'MVG']
# Official ImageMagick repositories, frozen by revision and compressed bytes.
SOURCES = {
    'Windows': ('8c38b33481e36f76b6d75596bda8c56a49b64ea5', '475908f4b12840f25bea5fc5f939d23ed723bd908831a5bb5532f787a1c0e12e'),
    'Configure': ('1214196e9e917f1d56bf5b6a3dc5ebe131970c18', 'd59ffd5e637db520a8a1651c9a89731b919245eb3eff2e3a5822b39c902cbcd9'),
    'Dependencies': ('1e18a4be8125d061c9925594628836bd33a45538', '137d3d1105efe25668ce95b950fba504e158d770ab3bc0302b35963342e17295'),
    'ImageMagick': ('refs/tags/'+VERSION, '1817720eae6d0b9e580ada17b53b5fc84a6e646a8830049451680ca9ee0b5cb5'),
    'bzip2': ('abffe764f875f71d051efb19d4c83139375f82d7', '7edce89fdcab36e334cc4c6fc89027bf914f47fe8706cf85a96638386beb0aed'),
    'freetype': ('6cf8b1a08bdec1d41246f51bcd12e5a62c9845e2', 'b8b548955e89ef38678648b388f4963565eea51b4f55a12a97f2a2a5eb4efc83'),
    'jpeg-turbo': ('b91492930ceb23c0b5282e8b9fc21de54182d92e', '7959c920b966cdc3843860f097fd4dae9a0a886916e4aad16acbe4c35315a28f'),
    'jpeg-turbo-12': ('9837071e8c0e5fba6daa03f31aba4416ff5eaf7c', '70f595dc9c0888eb5e35baa0a42d9a3177d5fedabff0420097f63e39607bd585'),
    'jpeg-turbo-16': ('d16d0839d655478b4d517ebe2aeabfecce8192d9', 'c9c7262427dd4299f83abddd80e4251bd8303bc3ec1ebeecbb9239cf523c4003'),
    'png': ('1ccb8b78643c7ad5360f00719a1e952ce0be7a53', '3834f1077e1bf700babee9d66053f6672c8bc239fc9160c1b896e8e347feb09a'),
    'webp': ('b981ef267195cb12f2cb97e4dd23e12a1323a4ce', '68cf734f51e8f28de43ef782f19cc29885ccdfc628897e46338cbfdfe08ba241'),
    'zlib': ('e42706934e4e7564d960031ae62ff29d918ee06d', 'f2529c29542276c87048dde2876d73310c16afc839f8a86e288712366696721d'),
}
PATCHED = {
    'Configure': {'src/stdafx.h', 'src/Config.h'},
    'ImageMagick': {'coders/coders-list.h', 'MagickCore/delegate.c', 'MagickCore/nt-base.c'},
    'freetype': {'src/base/ftsystem.c'},
}
GENERATED = {
    'Configure': {'Configs/package.version.h', 'Configs/version.h'},
    'ImageMagick': {'MagickCore/magick-baseconfig.h', 'MagickCore/version.h'},
}
GENERATOR_SOURCES = ['Config', 'Configs', 'InstallerConfig', 'Licence', 'MagickBaseConfig',
                     'Notice', 'Options', 'PerlMagick', 'Project', 'Projects',
                     'ResourceFiles', 'Solution', 'ThresholdMap', 'Version',
                     'VersionInfo', 'XmlConfigFiles']
HEADLESS = r'''
#include "Configs.h"
#include "Projects.h"
#include "Solution.h"
#include "MagickBaseConfig.h"
#include "VersionInfo.h"
#include "ThresholdMap.h"
#include "XmlConfigFiles.h"
#include "ResourceFiles.h"
int wmain(int argc, wchar_t **argv) {
  if (argc != 2) return 2;
  try {
    Options o(std::wstring(argv[1])+L"\\");
    o.checkImageMagickVersion();
    o.isStaticBuild=TRUE; o.useOpenCL=FALSE; o.useOpenMP=FALSE;
    o.enableDpc=FALSE; o.excludeDeprecated=FALSE; o.onlyMagick=TRUE;
    o.visualStudioVersion=VisualStudioVersion::VS2026;
    auto version=VersionInfo::load(o); auto configs=Configs::load(o);
    auto projects=Projects::create(o,configs);
    ResourceFiles::write(o,configs); Projects::write(projects); Solution::write(o,projects);
    if (version) version->write();
    MagickBaseConfig::write(o); XmlConfigFiles::write(o); ThresholdMap::write(o);
    return 0;
  } catch (const std::exception &error) {
    std::cerr << error.what() << std::endl; return 1;
  }
}
'''


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_changed(path, data):
    if isinstance(data, str):
        data = data.encode('utf-8')
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.is_file() or path.read_bytes() != data:
        path.write_bytes(data)


def source_archive(name, cache):
    revision, digest = SOURCES[name]
    filename = name+'-'+revision.rsplit('/', 1)[-1]+'.zip'
    archive = cache/filename
    url = 'https://codeload.github.com/ImageMagick/'+name+'/zip/'+revision
    if not archive.is_file():
        temporary = archive.with_suffix('.partial')
        print('Downloading '+name+' '+revision, flush=True)
        urllib.request.urlretrieve(url, temporary)
        if sha(temporary) != digest:
            raise ValueError('Pinned download checksum mismatch: '+url)
        temporary.replace(archive)
    if sha(archive) != digest:
        raise ValueError('Pinned cache checksum mismatch: '+str(archive))
    return archive, {'revision': revision, 'url': url, 'archive_sha256': digest}


def verify_extract(archive, destination, name):
    """Verify original files; restore only the explicit deterministic patches."""
    inputs = {}
    destination.mkdir(parents=True, exist_ok=True)
    with ZipFile(archive) as source:
        for entry in source.infolist():
            if entry.is_dir():
                continue
            parts = Path(entry.filename).parts
            if len(parts) < 2 or '..' in parts or Path(entry.filename).is_absolute():
                raise ValueError('Unsafe source archive entry: '+entry.filename)
            relative = Path(*parts[1:]).as_posix()
            target = destination/relative
            original = source.read(entry)
            inputs[relative] = hashlib.sha256(original).hexdigest()
            if relative in GENERATED.get(name, set()) and target.is_file():
                continue
            if target.is_file() and relative not in PATCHED.get(name, set()):
                if target.read_bytes() != original:
                    raise ValueError('Unexpected upstream source modification: '+str(target))
            else:
                write_changed(target, original)
            # Upstream version generation falls back to this timestamp when a
            # codeload checkout lacks .git. Keep it independent of extraction time.
            if relative == 'm4/version.m4':
                import datetime
                stamp = datetime.datetime(*entry.date_time, tzinfo=datetime.timezone.utc).timestamp()
                os.utime(target, (stamp, stamp))
    return inputs


def patches(source):
    config = source/'Configure/src/Config.h'
    write_changed(config, config.read_text(encoding='utf-8').replace('const set<wstring>& Config::nasmIncludes(', 'const set<wstring>& nasmIncludes('))
    stdafx = source/'Configure/src/stdafx.h'
    text = stdafx.read_text(encoding='utf-8')
    text = re.sub(r'#include <afxwin.h>.*?#include "resource.h"[^\n]*',
                  '#include <windows.h>\n#include <algorithm>\n#include <iomanip>\n#include <stdexcept>',
                  text, flags=re.S)
    if '<afx' in text:
        raise ValueError('Headless configure still includes MFC')
    write_changed(stdafx, text)
    write_changed(source/'Configure/src/headless.cpp', HEADLESS)
    coders = source/'ImageMagick/coders/coders-list.h'
    license_prefix = coders.read_text(encoding='utf-8').split('AddMagickCoder(', 1)[0]
    write_changed(coders, license_prefix+''.join('AddMagickCoder('+coder+')\n' for coder in CODERS))
    delegate = source/'ImageMagick/MagickCore/delegate.c'
    text = delegate.read_text(encoding='utf-8')
    text = '#pragma warning(disable: 4702) /* Intentionally short-circuited external delegates. */\n'+text
    for signature, result in [('MagickExport int ExternalDelegateCommand(', '-1'),
                              ('MagickExport MagickBooleanType InvokeDelegate(', 'MagickFalse')]:
        position = text.index('{', text.index(signature))+1
        text = text[:position]+'\n  /* DVD-Audio-Maker: external delegates are disabled. */\n  (void) ThrowMagickException(exception,GetMagickModule(),PolicyError,"NotAuthorized","external delegates are disabled");\n  return '+result+';\n'+text[position:]
    write_changed(delegate, text)
    ntbase = source/'ImageMagick/MagickCore/nt-base.c'
    text = ntbase.read_text(encoding='utf-8').replace(
        '  SetUnhandledExceptionFilter(NTUncaughtException);',
        '  /* DVD-Audio-Maker: preserve the embedding application exception handler. */')
    write_changed(ntbase, text)
    ft_system = source/'Dependencies/Dependencies/freetype/src/base/ftsystem.c'
    text = ft_system.read_text(encoding='utf-8')
    helper = '''
/* DVD-Audio-Maker: bundled and user-selected font paths are UTF-8. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
static FT_FILE *dvda_utf8_fopen(const char *path)
{
  int count=MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,path,-1,NULL,0);
  if (!count) return NULL;
  wchar_t *wide=malloc((size_t)count*sizeof(wchar_t));
  if (!wide) return NULL;
  if (!MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,path,-1,wide,count)) { free(wide); return NULL; }
  FT_FILE *file=_wfopen(wide,L"rb"); free(wide); return file;
}
'''
    anchor = '#include <freetype/fttypes.h>'
    text = text.replace(anchor, anchor+'\n'+helper)
    if 'ft_fopen( filepathname, "rb" )' not in text:
        raise ValueError('Pinned FreeType font I/O patch no longer matches')
    write_changed(ft_system, text.replace('ft_fopen( filepathname, "rb" )', 'dvda_utf8_fopen( filepathname )'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work-directory', type=Path, default=Path('build/msvc-native/image'))
    parser.add_argument('--source-directory', type=Path, help='Reuse an authenticated Windows source/build tree')
    parser.add_argument('--cache-directory', type=Path, help='Pinned source ZIP cache')
    parser.add_argument('--jobs', type=int, default=worker_count())
    parser.add_argument('--prepare-only', action='store_true')
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    work = args.work_directory.resolve(); work.mkdir(parents=True, exist_ok=True)
    source = (args.source_directory or work/'source').resolve()
    cache = (args.cache_directory or work/'downloads').resolve(); cache.mkdir(parents=True, exist_ok=True)
    sources = {}
    for name in SOURCES:
        archive, record = source_archive(name, cache)
        if name == 'Windows':
            folder = source
        elif name in ('Configure', 'Dependencies', 'ImageMagick'):
            folder = source/name
        else:
            folder = source/'Dependencies/Dependencies'/name
        record['original_files'] = verify_extract(archive, folder, name)
        sources[name] = record
    patches(source)
    patched = {}
    for name, names in PATCHED.items():
        folder = source/name if name in ('Configure', 'ImageMagick') else source/'Dependencies/Dependencies'/name
        for relative in names:
            path = folder/relative
            patched[path.relative_to(source).as_posix()] = sha(path)
    patched['Configure/src/headless.cpp'] = sha(source/'Configure/src/headless.cpp')
    (work/'source.json').write_text(json.dumps({'sources': sources, 'patches': patched}, indent=2)+'\n', encoding='utf-8')
    if args.prepare_only:
        print('Pinned MSVC image sources authenticated: '+str(source), flush=True)
        return
    env = msvc_environment()
    env['PATH'] = str(source/'Configure/Tools')+os.pathsep+env['PATH']
    commands = []

    def run(command, cwd, log_name):
        command = [str(value) for value in command]
        log = work/log_name
        with log.open('wb') as output:
            process = subprocess.run(command, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT)
        commands.append({'command': command, 'cwd': str(cwd), 'log': log_name, 'returncode': process.returncode})
        (work/'commands.json').write_text(json.dumps(commands, indent=2)+'\n', encoding='utf-8')
        if process.returncode:
            raise RuntimeError('MSVC image command failed; see '+str(log))

    configure = source/'Configure'
    run([env['CC'], '/nologo', '/std:c++20', '/EHsc', '/O2', '/MD', '/D_CRT_SECURE_NO_WARNINGS',
         '/I'+str(configure/'src'), *[configure/'src'/(name+'.cpp') for name in GENERATOR_SOURCES],
         configure/'src/headless.cpp', '/Fe'+str(configure/'headless.exe'), '/Fo'+str(configure)+os.sep,
         '/link', 'shlwapi.lib', 'version.lib'], configure, 'configure-build.log')
    (source/'Artifacts/bin').mkdir(parents=True, exist_ok=True)
    run([configure/'headless.exe', source], source, 'configure-generate.log')
    generated_patches = {}
    for project in (source/'ProjectFiles').rglob('*.vcxproj'):
        text = project.read_text(encoding='utf-8')
        text = text.replace('<Command>$(SolutionDir)Configure\\Tools\\nasm.exe',
                            '<Command>"$(SolutionDir)Configure\\Tools\\nasm.exe"')
        text = text.replace('>MultiThreadedDLL</RuntimeLibrary>', '>MultiThreaded</RuntimeLibrary>')
        # Building the library dependency graph alone avoids unused tools/fuzzers.
        write_changed(project, text)
        generated_patches[project.relative_to(source).as_posix()] = sha(project)
    msbuild = Path(env['VSINSTALLDIR'])/'MSBuild/Current/Bin/MSBuild.exe'
    solution_filter = source/'IM7.Static.x64.libraries.slnf'
    projects = sorted(path.relative_to(source).as_posix() for path in (source/'ProjectFiles/x64').glob('CORE_*/*.vcxproj')
                      if path.parent.name != 'CORE_Magick++')
    write_changed(solution_filter, json.dumps({'solution': {'path': 'IM7.Static.x64.sln', 'projects': projects}}, indent=2)+'\n')
    # Upstream solutions dispatch Build correctly, including all delegate and
    # codec project dependencies. /MP is limited to keep /m bounded as well.
    run([msbuild, solution_filter, '/nologo', '/t:Build', '/m:'+str(args.jobs),
         '/p:Configuration=Release', '/p:Platform=x64', '/p:PlatformToolset=v145',
         '/p:CL_MPCount=1', '/v:minimal'], source, 'image-msbuild.log')
    libs = source/'Artifacts/lib'
    core = work/'compile/MagickCore/.libs/MagickCore-7.Q16HDRI.lib'
    core.parent.mkdir(parents=True, exist_ok=True)
    # BZip2 is an internal FreeType delegate, carried by the combined core.
    run([env['DVDA_LIB_TOOL'], '/nologo', '/OUT:'+str(core),
         libs/'CORE_RL_MagickCore_.lib', libs/'CORE_RL_coders_.lib',
         libs/'CORE_RL_filters_.lib', libs/'CORE_RL_bzip2_.lib'], work, 'core-merge.log')
    staged = [core]
    copies = {
        'CORE_RL_MagickWand_.lib': 'compile/MagickWand/.libs/MagickWand-7.Q16HDRI.lib',
        'CORE_RL_freetype_.lib': 'freetype-minimal.lib',
        'CORE_RL_png_.lib': 'lib/png16.lib',
        'CORE_RL_zlib_.lib': 'lib/z.lib',
        'CORE_RL_webp_.lib': 'lib/webpdecoder.lib',
    }
    for name, relative in copies.items():
        destination = work/relative; destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(libs/name, destination); staged.append(destination)
    jpeg = work/'lib/jpeg.lib'
    run([env['DVDA_LIB_TOOL'], '/nologo', '/OUT:'+str(jpeg),
         libs/'CORE_RL_jpeg-turbo_.lib', libs/'CORE_RL_jpeg-turbo-12_.lib',
         libs/'CORE_RL_jpeg-turbo-16_.lib'], work, 'jpeg-merge.log')
    staged.append(jpeg)
    # The Windows delegate is one combined WebP archive. Both public link names
    # retain that archive; MSVC pulls each required member once by its symbols.
    webpmux = work/'lib/webpmux.lib'; shutil.copy2(libs/'CORE_RL_webp_.lib', webpmux); staged.append(webpmux)
    for name in ('MagickCore', 'MagickWand'):
        for header in (source/'ImageMagick'/name).rglob('*.h'):
            write_changed(work/'include'/name/header.relative_to(source/'ImageMagick'/name), header.read_bytes())
    for header in (source/'Artifacts/include').rglob('*.h'):
        write_changed(work/'include'/header.relative_to(source/'Artifacts/include'), header.read_bytes())
    # Native header lookup for existing callers; no stale MinGW header snapshot.
    for name in ('MagickCore', 'MagickWand'):
        for header in (work/'include'/name).rglob('*.h'):
            write_changed(work/'ImageMagick'/name/header.relative_to(work/'include'/name), header.read_bytes())
    for config in (source/'Artifacts/bin').glob('*.xml'):
        shutil.copy2(config, work/config.name)
    shutil.copy2(source/'ImageMagick/LICENSE', work/'NOTICE.txt')
    headers = {path.relative_to(work).as_posix(): sha(path) for path in sorted((work/'include').rglob('*.h'))}
    record = {'schema_version': 1, 'version': VERSION, 'target': TARGET, 'toolchain': 'msvc',
              'profile': 'static', 'crt': 'static', 'quantum_depth': 16, 'hdri': True, 'openmp': False, 'opencl': False,
              'external_delegates': False, 'coders': CODERS, 'source_directory': str(source),
              'sources': sources, 'patches': patched, 'generated_projects': generated_patches,
              'commands': commands, 'headers': headers,
              'files': {path.relative_to(work).as_posix(): {'sha256': sha(path), 'bytes': path.stat().st_size}
                        for path in staged},
              'builder_sha256': sha(__file__), 'environment_helper_sha256': sha(Path(__file__).with_name('msvc_env.py'))}
    (work/'image-build.json').write_text(json.dumps(record, indent=2)+'\n', encoding='utf-8')
    print('MSVC image libraries ready: '+str(work/'image-build.json'), flush=True)


if __name__ == '__main__':
    main()
