# -*- coding: utf-8 -*-
# 补丁1:删除半成品 interleave_*_merged 函数 + 修复 audio_close_merged 签名/声明。
# 已应用到 /opt/dvda-author(运行前会自动 .bak 备份)。
import io, shutil

audio_c = '/opt/dvda-author/src/audio.c'
audio2_h = '/opt/dvda-author/src/include/audio2.h'

# ---- 1. audio.c ----
with io.open(audio_c, encoding='utf-8', errors='replace') as f:
    s = f.read()
shutil.copy(audio_c, audio_c + '.bak')

lines = s.split('\n')

def delete_range(start, end):
    del lines[start - 1:end]  # 1-based inclusive

delete_range(1903, 1914)  # interleave_24_bit_sample_extended_merged
delete_range(1845, 1874)  # interleave_sample_extended_merged + 注释

s2 = '\n'.join(lines)
s2 = s2.replace(
    'interleave_24_bit_sample_extended_merged(info->channels, buffer_increment, buf);',
    'interleave_24_bit_sample_extended(info->channels, buffer_increment, buf);')
s2 = s2.replace(
    'interleave_sample_extended_merged(info->channels, buffer_increment, buf, globals);',
    'interleave_sample_extended(info->channels, buffer_increment, buf);')
s2 = s2.replace(
    'if (info->mergeflag) return audio_close_merged(info);',
    'if (info->mergeflag) return audio_close_merged(info, globals);')

with io.open(audio_c, 'w', encoding='utf-8') as f:
    f.write(s2)

# ---- 2. audio2.h:补 audio_close_merged 前置声明 ----
with io.open(audio2_h, encoding='utf-8') as f:
    h = f.read()
shutil.copy(audio2_h, audio2_h + '.bak')

anchor = 'int audio_close(fileinfo_t* info, globalData* );'
assert anchor in h, 'anchor not found in audio2.h'
h = h.replace(anchor, anchor + '\nint audio_close_merged(fileinfo_t* info, globalData* globals);')

with io.open(audio2_h, 'w', encoding='utf-8') as f:
    f.write(h)

print('FIX1 DONE')
