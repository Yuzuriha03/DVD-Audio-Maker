"""Independently validate the production Rust author, filesystems and audio.

Uses no C author runtime. FFmpeg is a read-only independent MLP decoder; integer
LPCM is decoded directly from the authored private-stream packets. Retains every
input, disc, image and log under a new output directory.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import uuid

import pycdlib

ROOT = Path(__file__).resolve().parents[2]
SECTOR = 2048
RATES = {48000: 0, 96000: 1, 192000: 2, 44100: 8, 88200: 9, 176400: 10}
CGA = {1: 0, 2: 1, 3: 7, 4: 3, 5: 9, 6: 12}


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def be16(data, offset):
    return struct.unpack_from('>H', data, offset)[0]


def be32(data, offset):
    return struct.unpack_from('>I', data, offset)[0]


def run(command, cwd, log, success=True):
    result = subprocess.run([str(part) for part in command], cwd=cwd,
                            capture_output=True, timeout=180)
    log.write_bytes(result.stdout + result.stderr)
    require((result.returncode == 0) == success,
            'Unexpected process status '+str(result.returncode)+'; see '+str(log))
    return result


def wav(path, bits, channels, rate=48000, frames=1000):
    width = 2 if bits == 16 else 3
    payload = bytearray()
    for sample in range(frames):
        for channel in range(channels):
            value = ((sample * 127 + channel * 419) % (1 << (bits - 1))) - (1 << (bits - 2))
            if bits == 20:
                value <<= 4
            payload.extend(value.to_bytes(width, 'little', signed=True))
    if bits == 20:
        fmt = struct.pack('<HHIIHHHHI', 0xfffe, channels, rate, rate*channels*width,
                          channels*width, 24, 22, 20, 0)
        fmt += bytes.fromhex('0100000000001000800000aa00389b71')
    else:
        fmt = struct.pack('<HHIIHH', 1, channels, rate, rate*channels*width,
                          channels*width, bits)
    chunks = b'fmt '+struct.pack('<I', len(fmt))+fmt+b'data'+struct.pack('<I', len(payload))+payload
    chunks += bytes(len(payload) & 1)
    path.write_bytes(b'RIFF'+struct.pack('<I', len(chunks)+4)+b'WAVE'+chunks)
    return {'path': path.name, 'bits': bits, 'channels': channels, 'rate': rate,
            'mlp': False, 'cga': CGA[channels], 'pcm': bytes(payload), 'new_title': False}


def mlp(directory, source, filename):
    destination = directory/filename
    shutil.copy2(source, destination)
    rate, bits, channels = [int(part) for part in source.name.removesuffix('.aligned.mlp').removesuffix('.mlp').split('_')]
    return {'path': filename, 'bits': bits, 'channels': channels, 'rate': rate,
            'mlp': True, 'cga': destination.read_bytes()[11] & 31, 'new_title': False}


def pcm(payload, bits, channels):
    # Decode DVD speaker groups rather than reproduce the packetizer's byte table.
    second = {1: [], 2: [], 3: [2], 4: [2, 3], 5: [3, 4],
              6: [3, 4, 5] if bits == 16 else [2, 3]}[channels]
    first = [channel for channel in range(channels) if channel not in second]
    size = channels * {16: 4, 20: 5, 24: 6}[bits]
    require(len(payload) % size == 0, 'LPCM sample pair alignment')
    result = bytearray()
    for offset in range(0, len(payload), size):
        pair = payload[offset:offset+size]
        values = [[0] * channels for _ in range(2)]
        start = 0
        for group in (second, first):
            for rank, (sample, channel) in enumerate((sample, channel) for sample in range(2) for channel in group):
                high = int.from_bytes(pair[start+rank*2:start+rank*2+2], 'big')
                value = high
                if bits == 24:
                    value = high << 8 | pair[start+4*len(group)+rank]
                elif bits == 20:
                    low = pair[start+4*len(group)+rank//2]
                    value = high << 8 | ((low >> 4 if rank % 2 == 0 else low & 15) << 4)
                values[sample][channel] = value
            start += len(group) * {16: 4, 20: 5, 24: 6}[bits]
        for frame in values:
            for value in frame:
                result.extend(value.to_bytes(2 if bits == 16 else 3, 'little'))
    return bytes(result)


def audio_payload(data, expected_codec):
    result = bytearray()
    require(len(data) % SECTOR == 0, 'AOB sector size')
    for offset in range(0, len(data), SECTOR):
        sector = data[offset:offset+SECTOR]
        require(sector[:4] == b'\0\0\1\xba', 'AOB pack start')
        pes = 32 if sector[14:18] == b'\0\0\1\xbb' else 14
        require(sector[pes:pes+4] == b'\0\0\1\xbd', 'AOB private PES start')
        end = pes + 6 + be16(sector, pes+4)
        private = pes + 9 + sector[pes+8]
        require(sector[private] == expected_codec, 'ATSI codec differs from AOB')
        start = private + 4 + be16(sector, private+2)
        require(start <= end <= SECTOR, 'AOB PES declared length')
        result.extend(sector[start:end])
    return bytes(result)


def filesystem(image, disc):
    expected = {path.relative_to(disc).as_posix(): path.read_bytes()
                for path in disc.rglob('*') if path.is_file()}
    mounted = pycdlib.PyCdlib()
    mounted.open(str(image))
    lbas = {}
    try:
        for namespace in ('iso', 'udf'):
            actual = {}
            for directory, _, files in mounted.walk(**{namespace+'_path': '/'}):
                for name in files:
                    path = directory.rstrip('/')+'/'+name
                    output = io.BytesIO()
                    mounted.get_file_from_iso_fp(output, **{namespace+'_path': path})
                    key = path.lstrip('/').removesuffix(';1')
                    actual[key] = output.getvalue()
                    if namespace == 'iso':
                        lbas[key] = mounted.get_record(iso_path=path).extent_location()
            require(actual == expected, namespace+' file contents differ from authored tree')
    finally:
        mounted.close()
    return lbas, len(expected)


def format_key(track):
    return tuple(track[key] for key in ('mlp', 'bits', 'rate', 'channels', 'cga'))


def validate(case, groups, decoder):
    disc, image = case/'disc', case/'disc.iso'
    lbas, file_count = filesystem(image, disc)
    audio = disc/'AUDIO_TS'
    require(not list(audio.glob('*.VOB')), 'No-menu disc contains unexpected video')
    amg = (audio/'AUDIO_TS.IFO').read_bytes()
    require(amg[:12] == b'DVDAUDIO-AMG' and amg == (audio/'AUDIO_TS.BUP').read_bytes(), 'AMG signature/backup')
    require(amg[0x3f] == len(groups) and be32(amg, 0xc0) == 0 and be32(amg, 0xcc) == 0, 'AMG groups/no-menu flags')
    amg_lba = lbas['AUDIO_TS/AUDIO_TS.IFO']
    require(amg_lba+be32(amg, 12)+1-len(amg)//SECTOR == lbas['AUDIO_TS/AUDIO_TS.BUP'], 'AMG physical backup address')
    samg = (audio/'AUDIO_PP.IFO').read_bytes()
    total = sum(map(len, groups))
    required_sectors = max(64, ((16+52*total+SECTOR-1)//SECTOR)*8)
    require(len(samg) == required_sectors*SECTOR, 'SAMG expanded allocation')
    matrix = len(samg)//8
    require(samg[:12] == b'DVDAUDIOSAPP' and be16(samg, 12) == total, 'SAMG signature/count')
    require(all(samg[copy*matrix:(copy+1)*matrix] == samg[:matrix] for copy in range(8)), 'SAMG eight exact copies')
    titles, flat, decoded = [], 0, []
    for group, tracks in enumerate(groups, 1):
        prefix = f'AUDIO_TS/ATS_{group:02}'
        atsi = (audio/f'ATS_{group:02}_0.IFO').read_bytes()
        require(atsi[:12] == b'DVDAUDIO-ATS' and atsi == (audio/f'ATS_{group:02}_0.BUP').read_bytes(), 'ATSI signature/backup')
        aob = b''.join(path.read_bytes() for path in sorted(audio.glob(f'ATS_{group:02}_*.AOB')))
        aob_lba = lbas[prefix+'_1.AOB']
        atsi_lba = lbas[prefix+'_0.IFO']
        require(atsi_lba+be32(atsi, 0xc4) == aob_lba, 'ATSI physical AOB start')
        require(atsi_lba+be32(atsi, 12)+1-len(atsi)//SECTOR == lbas[prefix+'_0.BUP'], 'ATSI physical backup address')
        expected_titles = []
        for rank, track in enumerate(tracks):
            if rank == 0 or track['new_title'] or format_key(track) != format_key(tracks[rank-1]):
                expected_titles.append([])
            expected_titles[-1].append((rank, track))
        require(be16(atsi, 0x800) == len(expected_titles), 'ATSI automatic/explicit title boundaries')
        formats = []
        for title in expected_titles:
            key = format_key(title[0][1])
            if key not in formats:
                formats.append(key)
        previous_last = -1
        for title_rank, title in enumerate(expected_titles, 1):
            entry = 0x808+(title_rank-1)*8
            start = 0x800+be32(atsi, entry+4)
            require(atsi[start+2] == len(title) == atsi[start+3], 'ATSI title track count')
            title_pts = 0
            for local, (rank, track) in enumerate(title):
                pts = start+16+20*local
                extent = start+16+20*len(title)+12*local
                row = 16+52*flat
                first, last = be32(atsi, extent+4), be32(atsi, extent+8)
                require(first == previous_last+1 and first <= last < len(aob)//SECTOR, 'ATSI contiguous global track range')
                previous_last = last
                format_rank = (be16(atsi, pts) >> 11) & 7
                require(format_rank == formats.index(format_key(track)), 'ATSI track audio format selection')
                attr = 0x100+16*format_rank
                bits, rate = {16: 0, 20: 1, 24: 2}[track['bits']], RATES[track['rate']]
                require(be16(atsi, attr) == int(track['mlp']) << 8, 'ATSI attribute codec')
                require(atsi[attr+2] >> 4 == bits and atsi[attr+3] >> 4 == rate and atsi[attr+4] == track['cga'], 'ATSI actual audio attributes')
                require(samg[row+2:row+4] == bytes([group, rank+1]), 'SAMG physical group/track order')
                require(be32(samg, row+40) == be32(samg, row+44) == aob_lba+first and be32(samg, row+48) == aob_lba+last, 'SAMG actual ISO extent')
                require(be32(samg, row+4) == be32(atsi, pts+6) and be32(samg, row+8) == be32(atsi, pts+10), 'ATSI/SAMG PTS fields')
                require(samg[row+17] >> 4 == bits and samg[row+18] >> 4 == rate and samg[row+19] == track['cga'], 'SAMG audio attributes')
                flag = (0xd8 if rank == 0 else 0x58) if track['mlp'] else ((0xc0 if track['channels'] > 2 else 0xc8) if rank == 0 else 0x40)
                require(samg[row+16] == flag, 'SAMG track codec/continuation flag')
                title_pts += be32(atsi, pts+10)
                payload = audio_payload(aob[first*SECTOR:(last+1)*SECTOR], 0xa1 if track['mlp'] else 0xa0)
                if track['mlp']:
                    original = case/track['path']
                    require(payload == original.read_bytes(), 'MLP elementary payload differs')
                    elementary = case/f'decode-{group}-{rank}.mlp'
                    elementary.write_bytes(payload)
                    outputs = []
                    for label, source in [('source', original), ('authored', elementary)]:
                        output = case/f'decode-{group}-{rank}-{label}.pcm'
                        run([decoder, '-v', 'error', '-i', source, '-f', 's32le', '-acodec', 'pcm_s32le', '-y', output], case, output.with_suffix('.log'))
                        outputs.append(output.read_bytes())
                    require(outputs[0] == outputs[1] and len(outputs[0]) > 0, 'Independent MLP decoded PCM differs')
                    decoded.append({'group': group, 'track': rank+1, 'codec': 'MLP', 'bytes': len(outputs[1])})
                else:
                    unpacked = pcm(payload, track['bits'], track['channels'])
                    require(unpacked == track['pcm'], 'Independent LPCM decoded samples differ')
                    decoded.append({'group': group, 'track': rank+1, 'codec': 'LPCM', 'bytes': len(unpacked)})
                flat += 1
            require(be32(atsi, start+4) == title_pts, 'ATSI title PTS sum')
            titles.append((group, title_rank, len(title), title_pts))
        require(previous_last+1 == len(aob)//SECTOR, 'ATSI last track covers complete AOB')
    first_table, second_table = be32(amg, 0xc4)*SECTOR, be32(amg, 0xc8)*SECTOR
    require(first_table+4+14*len(titles) <= second_table, 'Expanded AMG tables overlap')
    for table in (first_table, second_table):
        require(be16(amg, table) == len(titles), 'AMG total titles')
        require(table+4+14*len(titles) <= len(amg), 'AMG title table capacity')
        for rank, (group, title, tracks, pts) in enumerate(titles):
            row = table+4+14*rank
            require(amg[row+1] == tracks and be32(amg, row+4) == pts and amg[row+8:row+10] == bytes([group, title]), 'AMG title identity/duration')
            require(amg_lba+be32(amg, row+10) == lbas[f'AUDIO_TS/ATS_{group:02}_0.IFO'], 'AMG physical group address')
    return {'groups': len(groups), 'tracks': total, 'titles': len(titles), 'samg_sectors': required_sectors,
            'amg_sectors': len(amg)//SECTOR, 'iso_bytes': image.stat().st_size, 'iso_sha256': sha(image),
            'filesystem_files': file_count, 'iso_and_udf_payloads_equal': True, 'decoded': decoded}


def clean(case):
    require(not list(case.glob('.dvda-author-*')) and not list(case.glob('.dvda-iso-*')), 'Staging files remain after operation')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, default=ROOT/'build/rust-author-production')
    parser.add_argument('--ffmpeg', default=shutil.which('ffmpeg'))
    parser.add_argument('--mlp-fixtures', type=Path, default=ROOT/'build/formats-real-parity')
    parser.add_argument('--output', '--work-directory', dest='output', type=Path)
    args = parser.parse_args()
    runtime, fixtures = args.runtime.resolve(), args.mlp_fixtures.resolve()
    executable = runtime/'dvda-author-dev.exe'
    manifest = json.loads((runtime/'author-build.json').read_text(encoding='utf-8-sig'))
    require(manifest['implementation'] == 'rust' and manifest['project_c_author_compiled'] is False, 'Production manifest still uses C author')
    require(manifest['files'][executable.name]['sha256'] == sha(executable), 'Production executable checksum mismatch')
    require(manifest['author_library']['entry'] == 'dvda_core::author_runtime::execute', 'Unexpected production entry')
    vendor_inputs = {name: value for name, value in manifest['source_inputs'].items()
                     if name.startswith('menu-runtime/')}
    require(sha(runtime/'menu-build.json') == vendor_inputs['menu-runtime/menu-build.json'],
            'Staged menu provenance checksum mismatch')
    menu_manifest = json.loads((runtime/'menu-build.json').read_text(encoding='utf-8-sig'))
    require(menu_manifest['adapter'] == 'rust' and menu_manifest['linkage'] == 'static-vendor',
            'Production menu adapter is not the static Rust adapter')
    for name in ('libdvda_menu_spu_vendor.a', 'libdvda_menu_nav_vendor.a'):
        require(menu_manifest['files'][name]['sha256'] == vendor_inputs['menu-runtime/'+name],
                'Production menu archive provenance mismatch: '+name)
    for name, item in manifest['runtime_files'].items():
        require(sha(runtime/name) == item['sha256'], 'Production dependency checksum mismatch: '+name)
    require(not any(name.startswith('dvda-') or name == 'mlp_encoder.dll'
                    for name in manifest['files'][executable.name]['imports']), 'Production author imports a migrated project DLL')
    require(args.ffmpeg, 'An independent MLP decoder is required')
    output = (args.output or ROOT/'build'/('rust-author-acceptance-'+uuid.uuid4().hex[:8])).resolve()
    require(not output.exists() or not any(output.iterdir()), 'Use a new or empty acceptance directory')
    output.mkdir(parents=True, exist_ok=True)
    report = {'runtime': str(runtime), 'runtime_manifest_sha256': sha(runtime/'author-build.json'),
              'executable_sha256': sha(executable), 'project_c_author_used': False,
              'menu_vendor_inputs': vendor_inputs,
              'independent_decoder': str(args.ffmpeg), 'cases': [], 'failures': [], 'passed': False}

    def execute(case, groups, success=True, extra=()):
        command = [executable]
        for tracks in groups:
            command.append('-g')
            for track in tracks:
                if track.get('new_title'):
                    command.append('-z')
                command.append(track['path'])
        command += ['-o', 'disc', '-D', 'temp', '--iso=disc.iso', '--iso-volume=RUST_AUTHOR', *extra]
        return run(command, case, case/'author.log', success)

    case = output/'no-menu-unicode-inputs'
    case.mkdir()
    track = wav(case/'音楽 한글 🎵.wav', 16, 2)
    groups = [[track, track.copy()]]
    execute(case, groups)
    report['cases'].append({'case': case.name, **validate(case, groups, args.ffmpeg)})
    clean(case)

    case = output/'mixed-format-codec-groups'
    case.mkdir()
    stereo = wav(case/'stereo16.wav', 16, 2)
    high = wav(case/'stereo24.wav', 24, 2, 44100)
    surround20 = wav(case/'surround20.wav', 20, 6)
    surround24 = wav(case/'surround24.wav', 24, 6)
    m6 = mlp(case, fixtures/'96000_24_6.aligned.mlp', '六声道.mlp')
    m2 = mlp(case, fixtures/'96000_16_2.aligned.mlp', '二声道.mlp')
    forced = dict(m2, new_title=True)
    pcm96 = wav(case/'pcm96.wav', 16, 2, 96000)
    mono24 = wav(case/'mono24.wav', 24, 1, 96000)
    mono176 = wav(case/'mono176.wav', 16, 1, 176400)
    mono192 = wav(case/'mono192.wav', 16, 1, 192000)
    groups = [[stereo, stereo.copy(), high, surround20, m6, stereo.copy()],
              [m2, m2.copy(), forced, pcm96, surround24], [mono24, mono176, mono192]]
    execute(case, groups)
    report['cases'].append({'case': case.name, **validate(case, groups, args.ffmpeg)})
    clean(case)

    for count in (1, 9):
        case = output/f'{count}-groups-99-tracks'
        case.mkdir()
        tiny = wav(case/'x.wav', 16, 2, frames=2)
        groups = [[dict(tiny, new_title=rank > 0) for rank in range(99)] for _ in range(count)]
        execute(case, groups)
        report['cases'].append({'case': case.name, **validate(case, groups, args.ffmpeg)})
        clean(case)

    case = output/'9-groups-varied-title-layout'
    case.mkdir()
    tiny = wav(case/'x.wav', 16, 2, frames=2)
    groups = [[dict(tiny, new_title=rank > 0 and group % 2 == 0) for rank in range(99)]
              for group in range(9)]
    execute(case, groups)
    report['cases'].append({'case': case.name, **validate(case, groups, args.ffmpeg)})
    clean(case)

    for failure in ('truncated-wav', 'invalid-mlp-crc', 'occupied-output', 'iso-is-directory',
                    'iso-inside-output', 'temp-inside-output', 'iso-overwrites-input',
                    'nine-audio-formats', 'one-hundred-tracks', 'ten-groups'):
        case = output/failure
        case.mkdir()
        track = wav(case/'input.wav', 16, 2)
        old_image = b'previous authored ISO must remain unchanged\n'
        (case/'disc.iso').write_bytes(old_image)
        extra, keep = (), None
        if failure == 'truncated-wav':
            (case/'input.wav').write_bytes((case/'input.wav').read_bytes()[:-1])
        elif failure == 'invalid-mlp-crc':
            track = mlp(case, fixtures/'96000_24_6.aligned.mlp', 'invalid.mlp')
            data = bytearray((case/track['path']).read_bytes()); data[28] ^= 1
            (case/track['path']).write_bytes(data)
        elif failure == 'occupied-output':
            (case/'disc').mkdir(); (case/'disc/keep').write_bytes(b'previous disc')
            keep = case/'disc/keep'
        elif failure == 'iso-is-directory':
            (case/'disc.iso').unlink(); (case/'disc.iso').mkdir()
            (case/'disc.iso/keep').write_bytes(b'previous ISO directory')
            keep = case/'disc.iso/keep'
        elif failure == 'iso-inside-output':
            extra = ('--iso=disc/nested.iso',)
        elif failure == 'temp-inside-output':
            extra = ('-D', 'disc/temp')
        elif failure == 'iso-overwrites-input':
            extra = ('--iso=input.wav',)
        groups = [[track]]
        if failure == 'nine-audio-formats':
            groups = [[wav(case/f'format-{bits}-{channels}.wav', bits, channels, frames=2)
                       for bits, channels in [(16, channel) for channel in range(1, 7)]
                       + [(24, 1), (24, 2), (24, 3)]]]
        elif failure == 'one-hundred-tracks':
            groups = [[track] * 100]
        elif failure == 'ten-groups':
            groups = [[track] for _ in range(10)]
        input_before = (case/track['path']).read_bytes()
        keep_before = keep.read_bytes() if keep else None
        execute(case, groups, False, extra)
        require((case/track['path']).read_bytes() == input_before, 'Failure modified source audio')
        if failure != 'iso-is-directory':
            require((case/'disc.iso').read_bytes() == old_image, 'Failure replaced previous ISO')
        if keep:
            require(keep.read_bytes() == keep_before, 'Failure modified previous output')
        if failure != 'occupied-output':
            require(not (case/'disc').exists(), 'Failure committed partial authored tree')
        clean(case)
        report['failures'].append({'case': failure, 'previous_iso_and_input_preserved': True, 'staging_removed': True})
        if failure == 'truncated-wav':
            recovered = wav(case/'input.wav', 16, 2)
            execute(case, [[recovered]])
            report['cases'].append({'case': 'same-arguments-retry-after-failure', **validate(case, [[recovered]], args.ffmpeg)})
            clean(case)

    recorded = {name: value for name, value in manifest['source_inputs'].items()
                if name.startswith('rust/') or name.startswith('tools/win-build/')}
    stale = [name for name, value in recorded.items() if not (ROOT/name).is_file() or sha(ROOT/name) != value]
    report['source_inputs'] = recorded
    report['acceptance_source_inputs'] = {Path(__file__).relative_to(ROOT).as_posix(): sha(Path(__file__))}
    report['runtime_source_inputs_current'] = not stale
    report['stale_runtime_sources'] = stale
    report['passed'] = not stale
    (output/'acceptance.json').write_text(json.dumps(report, indent=2, ensure_ascii=False)+'\n', encoding='utf-8')
    require(not stale, 'Runtime sources changed after production build: '+', '.join(stale))
    print('Rust author acceptance passed: '+str(output/'acceptance.json'), flush=True)


if __name__ == '__main__':
    main()
