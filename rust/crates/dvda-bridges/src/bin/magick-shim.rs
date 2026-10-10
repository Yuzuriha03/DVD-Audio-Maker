use dvda_bridges::win::*;
fn fail(message: &str, code: u32) -> ! {
    eprintln!("{message}; Windows error {code}");
    std::process::exit(125)
}
fn main() {
    unsafe {
        let mut path = vec![0u16; 32768];
        let size = GetModuleFileNameW(std::ptr::null_mut(), path.as_mut_ptr(), 32768) as usize;
        if size == 0 || size >= 32768 {
            fail("Cannot locate the ImageMagick forwarder", GetLastError())
        }
        path.truncate(size);
        let directory = path
            .iter()
            .rposition(|&c| c == 92 || c == 47)
            .map_or(0, |i| i + 1);
        let name = String::from_utf16_lossy(&path[directory..]).to_ascii_lowercase();
        let mode = match name.as_str() {
            "convert.exe" => " convert ",
            "mogrify.exe" => " mogrify ",
            _ => fail(
                "The forwarder must be named convert.exe or mogrify.exe",
                123,
            ),
        };
        path.truncate(directory);
        path.extend("magick.exe".encode_utf16());
        path.push(0);
        let line = wtext(GetCommandLineW());
        let mut start = 0;
        while start < line.len() && (line[start] == 32 || line[start] == 9) {
            start += 1
        }
        let mut quoted = false;
        while start < line.len() {
            if line[start] == 34 {
                quoted = !quoted
            } else if !quoted && (line[start] == 32 || line[start] == 9) {
                break;
            }
            start += 1
        }
        while start < line.len() && (line[start] == 32 || line[start] == 9) {
            start += 1
        }
        let mut command = vec![34];
        command.extend_from_slice(&path[..path.len() - 1]);
        command.push(34);
        command.extend(mode.encode_utf16());
        command.extend_from_slice(&line[start..]);
        if command.len() >= 32768 {
            fail("ImageMagick argument list is too long", 24)
        }
        command.push(0);
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            fail("Cannot create the ImageMagick process job", GetLastError())
        }
        let mut limits: Job = std::mem::zeroed();
        limits.flags = 0x2000;
        if SetInformationJobObject(
            job,
            9,
            &limits as *const _ as *const _,
            std::mem::size_of::<Job>() as u32,
        ) == 0
        {
            fail(
                "Cannot configure the ImageMagick process job",
                GetLastError(),
            )
        }
        let mut startup: Startup = std::mem::zeroed();
        startup.cb = std::mem::size_of::<Startup>() as u32;
        startup.flags = 0x100;
        startup.input = GetStdHandle(-10i32 as u32);
        startup.output = GetStdHandle(-11i32 as u32);
        startup.error = GetStdHandle(-12i32 as u32);
        let mut child: Process = std::mem::zeroed();
        if CreateProcessW(
            path.as_ptr(),
            command.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            0x08000004,
            std::ptr::null(),
            std::ptr::null(),
            &mut startup,
            &mut child,
        ) == 0
        {
            fail("Cannot launch bundled magick.exe", GetLastError())
        }
        if AssignProcessToJobObject(job, child.process) == 0 {
            let e = GetLastError();
            TerminateProcess(child.process, 125);
            fail("Cannot supervise the ImageMagick child", e)
        }
        if ResumeThread(child.thread) == u32::MAX {
            let e = GetLastError();
            TerminateProcess(child.process, 125);
            fail("Cannot resume the ImageMagick child", e)
        }
        CloseHandle(child.thread);
        let mut code = 125;
        if WaitForSingleObject(child.process, u32::MAX) != 0
            || GetExitCodeProcess(child.process, &mut code) == 0
        {
            fail("Cannot obtain the ImageMagick result", GetLastError())
        }
        CloseHandle(child.process);
        CloseHandle(job);
        std::process::exit(code as i32)
    }
}
