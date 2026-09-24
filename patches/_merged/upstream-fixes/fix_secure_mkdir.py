# -*- coding: utf-8 -*-
# 补丁3:secure_mkdir 空路径死循环 → 改为返回 -1(而非无限递归自身)。
import io, shutil

p = '/opt/dvda-author/libutils/src/libc_utils.c'

with io.open(p, encoding='utf-8', errors='replace') as f:
    s = f.read()
shutil.copy(p, p + '.bak')

old = '''    if (path == NULL || path[0]=='\\0')
    {
     fprintf(stderr, "%s%s%s","\\n"ERR "Could not create directory with empty or null path. Using temporary directory : ", globals->settings.tempdir, "\\n");
     return secure_mkdir(path, globals->access_rights, globals);
    }
'''

new = '''    if (path == NULL || path[0]=='\\0')
    {
     fprintf(stderr, "%s%s%s","\\n"ERR "Could not create directory with empty or null path. Using temporary directory : ", globals->settings.tempdir, "\\n");
     return -1;
    }
'''

assert old in s, 'secure_mkdir bug block not found'
s = s.replace(old, new, 1)

with io.open(p, 'w', encoding='utf-8') as f:
    f.write(s)

print('FIX3 DONE')
