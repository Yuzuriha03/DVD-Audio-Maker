//! Windows child process ownership. Launch suspended, join a kill-on-close job,
//! then resume; cancellation cannot miss a descendant created during startup.
use std::{
    ffi::{OsStr, OsString, c_void},
    io,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::Path,
    ptr,
};
type Handle = *mut c_void;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn MultiByteToWideChar(
        code_page: u32,
        flags: u32,
        input: *const u8,
        input_size: i32,
        output: *mut u16,
        output_size: i32,
    ) -> i32;
}
/// Decode a completed line. UTF-16/32 are handled explicitly because Windows
/// code-page conversion does not implement these .NET encoding identifiers.
pub fn decode(bytes: &[u8], code_page: u32) -> io::Result<String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    if code_page == 65001 {
        return Ok(String::from_utf8_lossy(bytes).into_owned());
    }
    if code_page == 1200 || code_page == 1201 {
        let mut value: String = char::decode_utf16(bytes.as_chunks::<2>().0.iter().map(|b| {
            if code_page == 1200 {
                u16::from_le_bytes([b[0], b[1]])
            } else {
                u16::from_be_bytes([b[0], b[1]])
            }
        }))
        .map(|v| v.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
        if !bytes.len().is_multiple_of(2) {
            value.push(char::REPLACEMENT_CHARACTER);
        }
        return Ok(value);
    }
    if code_page == 12000 || code_page == 12001 {
        let mut value: String = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| {
                let units = *b;
                char::from_u32(if code_page == 12000 {
                    u32::from_le_bytes(units)
                } else {
                    u32::from_be_bytes(units)
                })
                .unwrap_or(char::REPLACEMENT_CHARACTER)
            })
            .collect();
        if !bytes.len().is_multiple_of(4) {
            value.push(char::REPLACEMENT_CHARACTER);
        }
        return Ok(value);
    }
    if code_page == 20127 {
        return Ok(bytes
            .iter()
            .map(|b| if *b < 128 { *b as char } else { '?' })
            .collect());
    }
    if code_page == 28591 {
        return Ok(bytes.iter().map(|b| char::from(*b)).collect());
    }
    let length = i32::try_from(bytes.len())
        .map_err(|_| io::Error::other("Child output line is too large"))?;
    let count =
        unsafe { MultiByteToWideChar(code_page, 0, bytes.as_ptr(), length, ptr::null_mut(), 0) };
    if count == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut units = vec![0u16; count as usize];
    if unsafe {
        MultiByteToWideChar(
            code_page,
            0,
            bytes.as_ptr(),
            length,
            units.as_mut_ptr(),
            count,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(String::from_utf16_lossy(&units))
}
#[repr(C)]
struct Security {
    size: u32,
    descriptor: Handle,
    inherit: i32,
}
#[repr(C)]
#[derive(Default)]
struct Startup {
    size: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    chars_x: u32,
    chars_y: u32,
    fill: u32,
    flags: u32,
    show: u16,
    reserved_size: u16,
    reserved_bytes: *mut u8,
    stdin: Handle,
    stdout: Handle,
    stderr: Handle,
}
#[repr(C)]
struct StartupEx {
    base: Startup,
    attributes: Handle,
}
#[repr(C)]
#[derive(Default)]
struct ProcessInfo {
    process: Handle,
    thread: Handle,
    process_id: u32,
    thread_id: u32,
}
#[repr(C)]
#[derive(Default)]
struct Limits {
    process_time: i64,
    job_time: i64,
    flags: u32,
    working_min: usize,
    working_max: usize,
    active: u32,
    affinity: usize,
    priority: u32,
    scheduling: u32,
    io: [u64; 6],
    process_memory: usize,
    job_memory: usize,
    peak_process: usize,
    peak_job: usize,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CloseHandle(handle: Handle) -> i32;
    fn CreatePipe(
        read: *mut Handle,
        write: *mut Handle,
        security: *const Security,
        size: u32,
    ) -> i32;
    fn SetHandleInformation(handle: Handle, mask: u32, flags: u32) -> i32;
    fn CreateFileW(
        path: *const u16,
        access: u32,
        share: u32,
        security: *const Security,
        creation: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn CreateJobObjectW(security: *const Security, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, length: u32) -> i32;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    fn TerminateJobObject(job: Handle, code: u32) -> i32;
    fn TerminateProcess(process: Handle, code: u32) -> i32;
    fn InitializeProcThreadAttributeList(
        list: Handle,
        count: u32,
        flags: u32,
        size: *mut usize,
    ) -> i32;
    fn UpdateProcThreadAttribute(
        list: Handle,
        flags: u32,
        attribute: usize,
        value: *mut c_void,
        size: usize,
        previous: Handle,
        returned: *mut usize,
    ) -> i32;
    fn DeleteProcThreadAttributeList(list: Handle);
    fn CreateProcessW(
        application: *const u16,
        command: *mut u16,
        process_security: *const Security,
        thread_security: *const Security,
        inherit: i32,
        flags: u32,
        environment: Handle,
        directory: *const u16,
        startup: *const StartupEx,
        info: *mut ProcessInfo,
    ) -> i32;
    fn ResumeThread(thread: Handle) -> u32;
    fn PeekNamedPipe(
        pipe: Handle,
        data: *mut c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        left: *mut u32,
    ) -> i32;
    fn ReadFile(
        file: Handle,
        data: *mut c_void,
        size: u32,
        read: *mut u32,
        overlapped: Handle,
    ) -> i32;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn GetExitCodeProcess(process: Handle, code: *mut u32) -> i32;
    fn SearchPathW(
        path: *const u16,
        file: *const u16,
        extension: *const u16,
        length: u32,
        result: *mut u16,
        part: *mut *mut u16,
    ) -> u32;
    fn CompareStringOrdinal(
        left: *const u16,
        left_length: i32,
        right: *const u16,
        right_length: i32,
        ignore_case: i32,
    ) -> i32;
}
struct Owned(Handle);
impl Owned {
    fn new(value: Handle) -> io::Result<Self> {
        if value.is_null() || value as isize == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(value))
        }
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Attributes {
    storage: Vec<usize>,
}
impl Attributes {
    fn pointer(&self) -> Handle {
        self.storage.as_ptr().cast_mut().cast()
    }
    fn new(handles: &mut [Handle]) -> io::Result<Self> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        if unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 1, 0, &mut bytes)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let attributes = Self { storage };
        if unsafe {
            UpdateProcThreadAttribute(
                attributes.pointer(),
                0,
                0x20002,
                handles.as_mut_ptr().cast(),
                size_of_val(handles),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(attributes)
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.pointer());
        }
    }
}
fn security() -> Security {
    Security {
        size: size_of::<Security>() as u32,
        descriptor: ptr::null_mut(),
        inherit: 1,
    }
}
fn pipe() -> io::Result<(Owned, Owned)> {
    let mut read = ptr::null_mut();
    let mut write = ptr::null_mut();
    if unsafe { CreatePipe(&mut read, &mut write, &security(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let read = Owned::new(read)?;
    let write = Owned::new(write)?;
    if unsafe { SetHandleInformation(read.0, 1, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((read, write))
}
fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut value: Vec<_> = value.encode_wide().collect();
    if value.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "NUL in process argument",
        ));
    }
    value.push(0);
    Ok(value)
}
fn equal_key(left: &OsStr, right: &OsStr) -> bool {
    let left: Vec<_> = left.encode_wide().collect();
    let right: Vec<_> = right.encode_wide().collect();
    unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            left.len() as i32,
            right.as_ptr(),
            right.len() as i32,
            1,
        ) == 2
    }
}
fn environment(overrides: &[(String, Option<String>)]) -> io::Result<Vec<u16>> {
    let mut entries: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    for (key, value) in overrides {
        if key.is_empty()
            || key.contains(['=', '\0'])
            || value.as_ref().is_some_and(|v| v.contains('\0'))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid process environment entry",
            ));
        }
        entries.retain(|(name, _)| !equal_key(name, OsStr::new(key)));
        if let Some(value) = value {
            entries.push((key.into(), value.into()));
        }
    }
    entries.sort_by(|(a, _), (b, _)| {
        let a: Vec<_> = a.encode_wide().collect();
        let b: Vec<_> = b.encode_wide().collect();
        unsafe { CompareStringOrdinal(a.as_ptr(), a.len() as i32, b.as_ptr(), b.len() as i32, 1) }
            .cmp(&2)
    });
    let mut block = Vec::new();
    for (key, value) in entries {
        block.extend(key.encode_wide());
        block.push('=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}
fn quote(argument: &str) -> String {
    let mut output = String::from("\"");
    let mut slashes = 0;
    for c in argument.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            '\\',
            slashes * if c == '"' { 2 } else { 1 },
        ));
        slashes = 0;
        if c == '"' {
            output.push('\\');
        }
        output.push(c);
    }
    output.extend(std::iter::repeat_n('\\', slashes * 2));
    output.push('"');
    output
}
fn executable(file: &str) -> io::Result<Vec<u16>> {
    let file = wide(OsStr::new(file))?;
    let extension: Vec<u16> = ".exe\0".encode_utf16().collect();
    let mut buffer = vec![0u16; 32768];
    let length = unsafe {
        SearchPathW(
            ptr::null(),
            file.as_ptr(),
            extension.as_ptr(),
            buffer.len() as u32,
            buffer.as_mut_ptr(),
            ptr::null_mut(),
        )
    };
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    if length as usize >= buffer.len() {
        return Err(io::Error::from_raw_os_error(206));
    }
    buffer.truncate(length as usize + 1);
    Ok(buffer)
}
pub struct Launch<'a> {
    pub file: &'a str,
    pub arguments: &'a [String],
    pub directory: Option<&'a Path>,
    pub environment: &'a [(String, Option<String>)],
}
pub enum ReadState {
    Data(usize),
    Empty,
    Closed,
}
pub struct Child {
    process: Owned,
    job: Owned,
    pipes: [Owned; 2],
}
impl Child {
    pub fn spawn(request: Launch<'_>) -> io::Result<Self> {
        let application = executable(request.file)?;
        let argv0 = OsString::from_wide(&application[..application.len() - 1])
            .to_string_lossy()
            .into_owned();
        let mut command = quote(&argv0);
        for argument in request.arguments {
            command.push(' ');
            command.push_str(&quote(argument));
        }
        let mut command = wide(OsStr::new(&command))?;
        if command.len() > 32767 {
            return Err(io::Error::from_raw_os_error(206));
        }
        let directory = request.directory.map(|p| wide(p.as_os_str())).transpose()?;
        let mut environment = environment(request.environment)?;
        let (output, output_write) = pipe()?;
        let (error, error_write) = pipe()?;
        let nul = wide(OsStr::new("NUL"))?;
        let input = Owned::new(unsafe {
            CreateFileW(
                nul.as_ptr(),
                0x80000000,
                3,
                &security(),
                3,
                0,
                ptr::null_mut(),
            )
        })?;
        let mut inherited = [input.0, output_write.0, error_write.0];
        let attributes = Attributes::new(&mut inherited)?;
        let startup = StartupEx {
            base: Startup {
                size: size_of::<StartupEx>() as u32,
                flags: 0x100,
                stdin: input.0,
                stdout: output_write.0,
                stderr: error_write.0,
                ..Startup::default()
            },
            attributes: attributes.pointer(),
        };
        let job = Owned::new(unsafe { CreateJobObjectW(ptr::null(), ptr::null()) })?;
        let limits = Limits {
            flags: 0x2000,
            ..Limits::default()
        };
        if unsafe {
            SetInformationJobObject(
                job.0,
                9,
                (&limits as *const Limits).cast(),
                size_of::<Limits>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut info = ProcessInfo::default();
        if unsafe {
            CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                1,
                0x08000000 | 0x00080000 | 0x400 | 4,
                environment.as_mut_ptr().cast(),
                directory.as_ref().map_or(ptr::null(), |v| v.as_ptr()),
                &startup,
                &mut info,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let process = Owned::new(info.process)?;
        let thread = Owned::new(info.thread)?;
        if unsafe { AssignProcessToJobObject(job.0, process.0) } == 0 {
            let error = io::Error::last_os_error();
            unsafe {
                TerminateProcess(process.0, 1);
                WaitForSingleObject(process.0, 5000);
            }
            return Err(error);
        }
        let child = Self {
            process,
            job,
            pipes: [output, error],
        };
        if unsafe { ResumeThread(thread.0) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(child)
    }
    pub fn read(&mut self, stream: usize, buffer: &mut [u8]) -> io::Result<ReadState> {
        if stream >= 2 || buffer.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid child pipe read",
            ));
        }
        let mut available = 0;
        if unsafe {
            PeekNamedPipe(
                self.pipes[stream].0,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                &mut available,
                ptr::null_mut(),
            )
        } == 0
        {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(109) {
                Ok(ReadState::Closed)
            } else {
                Err(error)
            };
        }
        if available == 0 {
            return Ok(ReadState::Empty);
        }
        let count = (available as usize).min(buffer.len());
        let mut read = 0;
        if unsafe {
            ReadFile(
                self.pipes[stream].0,
                buffer.as_mut_ptr().cast(),
                count as u32,
                &mut read,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(ReadState::Data(read as usize))
    }
    pub fn exit_code(&self) -> io::Result<Option<i32>> {
        match unsafe { WaitForSingleObject(self.process.0, 0) } {
            0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.process.0, &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(code as i32))
            }
            258 => Ok(None),
            _ => Err(io::Error::last_os_error()),
        }
    }
    pub fn wait_briefly(&self) {
        unsafe {
            WaitForSingleObject(self.process.0, 10);
        }
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        unsafe {
            TerminateJobObject(self.job.0, 1);
            WaitForSingleObject(self.process.0, 5000);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_abi() {
        assert_eq!(size_of::<Startup>(), 104);
        assert_eq!(size_of::<StartupEx>(), 112);
        assert_eq!(size_of::<ProcessInfo>(), 24);
        assert_eq!(size_of::<Security>(), 24);
        assert_eq!(size_of::<Limits>(), 144);
        assert_eq!(std::mem::offset_of!(Limits, flags), 16);
        assert_eq!(std::mem::offset_of!(Startup, stdin), 80);
    }
}
