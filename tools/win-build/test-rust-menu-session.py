"""Real vendor-session recovery, resource cleanup and production CLI acceptance.

The fixed MPEG source and synthetic RGBA callback make repeated SPU/navigation
byte comparisons independent of encoder timestamps. Vendor calls are synchronous;
author callback cancellation is checked separately at the application boundary.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import uuid
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def run(command, cwd, log, env=None, success=True):
    with log.open('wb') as output:
        result = subprocess.run([str(part) for part in command], cwd=cwd, env=env,
                                stdout=output, stderr=subprocess.STDOUT, timeout=180)
    require((result.returncode == 0) == success, 'Unexpected status '+str(result.returncode)+'; see '+str(log))


def tree(root):
    return {path.relative_to(root).as_posix(): path.read_bytes() for path in root.rglob('*') if path.is_file()}


def current_menu_inputs(record):
    inputs = dict(record['rust_inputs'])
    inputs.update({'tools/menu-native/'+name: value for name, value in record['source_inputs'].items()})
    return inputs


def authenticate(inputs):
    return [name for name, digest in inputs.items() if not (ROOT/name).is_file() or sha(ROOT/name) != digest]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--vendor', type=Path, default=ROOT/'build/menu-direct-vendor-rust-session')
    parser.add_argument('--runtime', type=Path, default=ROOT/'build/rust-author-production')
    parser.add_argument('--source-mpeg', type=Path, default=ROOT/'build/menu-production-pal-1/背景.mpg')
    parser.add_argument('--sessions-only', action='store_true', help='Validate the new vendor before production rebuilding; record production_checked=false')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    output = (args.output or ROOT/'build'/('menu-session-acceptance-'+uuid.uuid4().hex[:8])).resolve()
    require(not output.exists() or not any(output.iterdir()), 'Use a new or empty acceptance directory')
    output.mkdir(parents=True, exist_ok=True)
    vendor, runtime = args.vendor.resolve(), args.runtime.resolve()
    source = args.source_mpeg.resolve()
    require(source.is_file(), 'Provide a fixed sector-aligned PAL/DVD MPEG test source')
    require(source.stat().st_size > 0 and source.stat().st_size % 2048 == 0, 'MPEG fixture must be sector aligned')
    record = json.loads((vendor/'menu-build.json').read_text(encoding='utf-8-sig'))
    require(record['adapter'] == 'rust' and record['linkage'] == 'static-vendor', 'Unexpected migrated menu vendor provenance')
    for name in ('libdvda_menu_spu_vendor.a', 'libdvda_menu_nav_vendor.a'):
        require(sha(vendor/name) == record['files'][name]['sha256'], 'Vendor archive checksum mismatch: '+name)
    inputs = current_menu_inputs(record)
    stale = authenticate(inputs)
    require(not stale, 'New vendor sources changed since build: '+', '.join(stale))
    env = os.environ | {'DVDA_MENU_NATIVE_DIR': str(vendor)}
    target = output/'harness-target'
    run(['cargo', 'build', '--offline', '--manifest-path', ROOT/'rust/Cargo.toml', '-p', 'dvda-menu',
         '--features', 'direct-link', '--example', 'session-acceptance', '--target-dir', target], ROOT, output/'cargo.log', env)
    current = target/'debug/examples/session-acceptance.exe'
    sessions = {}
    for name in ('rust', 'repeat'):
        case = output/name/'中文-日本語 한글'
        run([current, case, source], ROOT, output/(name+'-session.log'))
        sessions[name] = json.loads((case/'session-report.json').read_text(encoding='utf-8'))
        require(sessions[name]['passed'], name+' session acceptance failed')
    new, repeated = [output/name/'中文-日本語 한글' for name in ('rust', 'repeat')]
    require((new/'menu.mpg').read_bytes() == (repeated/'menu.mpg').read_bytes(), 'SPU output differs across fresh Rust processes')
    new_nav, repeated_nav = [tree(case/'导航 日本語') for case in (new, repeated)]
    require(new_nav and new_nav == repeated_nav, 'Navigation output differs across fresh Rust processes')
    if args.sessions_only:
        require(not authenticate(inputs), 'New vendor sources changed during acceptance')
        report = {'passed': True, 'scope': 'menu sessions', 'production_checked': False,
                  'runtime_source_inputs_current': True, 'vendor_calls_synchronous': True,
                  'vendor_cancellation_hook': False, 'reference': 'two fresh Rust processes',
                  'vendor': str(vendor), 'harness_sha256': sha(current),
                  'vendor_manifest_sha256': sha(vendor/'menu-build.json'),
                  'source_mpeg_sha256': sha(source), 'spu_repeatable': True,
                  'navigation_repeatable': True, 'sessions': sessions, 'menu_source_inputs': inputs,
                  'acceptance_source_inputs': {name: sha(ROOT/name) for name in [
                      'tools/win-build/test-rust-menu-session.py', 'rust/crates/dvda-menu/examples/session-acceptance.rs']}}
        (output/'acceptance.json').write_text(json.dumps(report, indent=2, ensure_ascii=False)+'\n', encoding='utf-8')
        print('Rust menu session acceptance passed (production pending): '+str(output/'acceptance.json'), flush=True)
        return
    # Use the actual assembled production runtime for codecs, image callbacks,
    # menu entry and filesystem production. Check payloads rather than compare
    # images from separate MPEG encodes byte for byte.
    author = runtime/'dvda-author-dev.exe'
    production_record = json.loads((runtime/'author-build.json').read_text(encoding='utf-8-sig'))
    require(sha(author) == production_record['files'][author.name]['sha256'], 'Production executable checksum mismatch')
    require(production_record['implementation'] == 'rust' and not production_record['project_c_author_compiled'], 'Production still uses the C author')
    for name, item in production_record['runtime_files'].items():
        require(sha(runtime/name) == item['sha256'], 'Production dependency checksum mismatch: '+name)
    for name in ('menu-build.json', 'libdvda_menu_spu_vendor.a', 'libdvda_menu_nav_vendor.a'):
        require(production_record['source_inputs']['menu-runtime/'+name] == sha(vendor/name), 'Production links a different menu vendor: '+name)
    spec = importlib.util.spec_from_file_location('menu_acceptance', ROOT/'tools/win-build/test-rust-author-menu.py')
    menu_acceptance = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(menu_acceptance)
    # A missing bundle dependency must not be supplied by an unrelated developer
    # directory. Keep only Windows system locations in the production child PATH.
    production_env = {name: value for name, value in os.environ.items()
                      if not name.startswith(('DVDA_', 'MAGICK_'))}
    system = Path(os.environ.get('SystemRoot', 'C:/Windows'))
    production_env['PATH'] = str(system/'System32')+os.pathsep+str(system)
    run([author, '--version'], output, output/'production-version.log', production_env)
    production = output/'production 中文-日本語'
    production.mkdir()
    previous_env = dict(os.environ)
    try:
        os.environ.clear(); os.environ.update(production_env)
        checks = menu_acceptance.run_case(runtime, production, 'pal', 1, None)
    finally:
        os.environ.clear(); os.environ.update(previous_env)
    retry = output/'production-failure-retry 中文'
    retry.mkdir()
    input_path = retry/'input.wav'
    menu_acceptance.wav(input_path, 1)
    nav_xml = retry/'导航.xml'
    source_menu = new/'menu.mpg'
    text = '<dvdauthor jumppad="1"><amgm><menus><video format="pal"/><audio format="mp2" lang="en"/><pgc><pre>g0=1;</pre><button name="button01">jump menu 1;</button><vob file="'+escape(str(source_menu), {'"':'&quot;'})+'"/><post>jump menu 1;</post></pgc></menus></amgm></dvdauthor>'
    nav_xml.write_text(text.replace('g0=1;', 'not_a_vm_command;'), encoding='utf-8')
    old_image = b'previous ISO must survive menu failure\n'
    (retry/'disc.iso').write_bytes(old_image)
    command = [author, '-g', input_path, '-o', retry/'disc', '-D', retry/'temporary',
               '--topmenu='+str(source_menu), '--xml', nav_xml, '--iso='+str(retry/'disc.iso')]
    run(command, retry, retry/'failure.log', production_env, success=False)
    require(not (retry/'disc').exists() and (retry/'disc.iso').read_bytes() == old_image, 'Failed menu author committed output or changed the previous ISO')
    require(not list(retry.glob('.dvda-author-*')) and not list(retry.glob('.dvda-iso-*')), 'Failed menu author left staging files')
    nav_xml.write_text(text, encoding='utf-8')
    run(command, retry, retry/'retry.log', production_env)
    menu_acceptance.filesystem(retry/'disc.iso', retry/'disc')
    # No-menu unit cases independently cover cancellation before and during ISO,
    # rollback, existing-image protection and retry. This does not claim a
    # cancellation hook inside a synchronous third-party menu core.
    run(['cargo', 'test', '--offline', '--manifest-path', ROOT/'rust/Cargo.toml', '-p', 'dvda-author',
         '--test', 'full_author_large', '--target-dir', output/'author-cancel-target'], ROOT, output/'author-cancellation.log')
    stale = authenticate(inputs)
    production_inputs = {name: digest for name, digest in production_record['source_inputs'].items()
                         if name.startswith('rust/') or name.startswith('tools/win-build/')}
    production_stale = authenticate(production_inputs)
    require(not stale and not production_stale, 'Source inputs changed during menu acceptance')
    report = {'passed': True, 'scope': 'menu sessions and production', 'production_checked': True,
              'runtime_source_inputs_current': True,
              'vendor_calls_synchronous': True, 'vendor_cancellation_hook': False,
              'author_callback_cancellation_tests_passed': True, 'reference': 'two fresh Rust processes',
              'vendor': str(vendor), 'harness_sha256': sha(current), 'production_executable_sha256': sha(author),
              'vendor_manifest_sha256': sha(vendor/'menu-build.json'),
              'source_mpeg_sha256': sha(source), 'spu_repeatable': True,
              'navigation_repeatable': True, 'navigation_files': {name: hashlib.sha256(data).hexdigest() for name, data in new_nav.items()},
              'sessions': sessions, 'production_menu': checks, 'production_failure_retry': True,
              'menu_source_inputs': inputs, 'production_source_inputs': production_inputs,
              'acceptance_source_inputs': {name: sha(ROOT/name) for name in [
                  'tools/win-build/test-rust-menu-session.py', 'tools/win-build/test-rust-author-menu.py',
                  'rust/crates/dvda-menu/examples/session-acceptance.rs', 'rust/crates/dvda-author/tests/full_author_large.rs']}}
    (output/'acceptance.json').write_text(json.dumps(report, indent=2, ensure_ascii=False)+'\n', encoding='utf-8')
    print('Rust menu session acceptance passed: '+str(output/'acceptance.json'), flush=True)


if __name__ == '__main__':
    main()
