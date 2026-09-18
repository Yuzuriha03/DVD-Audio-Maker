# -*- coding: utf-8 -*-
# 补丁2:winport.h 补跨平台 close_handles() 实现(上游遗漏)。
import io, shutil

p = '/opt/dvda-author/libutils/src/include/winport.h'

with io.open(p, encoding='utf-8', errors='replace') as f:
    s = f.read()
shutil.copy(p, p + '.bak')

func = ''' static inline void close_handles(FILE_DESCRIPTOR a, FILE_DESCRIPTOR b, FILE_DESCRIPTOR c, FILE_DESCRIPTOR d)
{
#ifdef _WIN32
    CloseHandle(a); CloseHandle(b); CloseHandle(c); CloseHandle(d);
#else
    close(a); close(b); close(c); close(d);
#endif
}

'''

anchor = ' void  pipe_to_child_stdin(const char* name,'
assert anchor in s, 'anchor not found in winport.h'
s = s.replace(anchor, func + anchor, 1)

with io.open(p, 'w', encoding='utf-8') as f:
    f.write(s)

print('FIX2 DONE')
