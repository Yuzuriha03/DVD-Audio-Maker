use dvda_core::process::{Job, Outcome, execute};
use dvda_native::media::Callbacks;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
#[derive(Default)]
struct Events {
    output: Vec<String>,
    error: Vec<String>,
    cancel: bool,
    cancel_on_ready: bool,
    panic_on_ready: bool,
}
impl Callbacks for Events {
    fn emit(&mut self, stream: i32, text: &str) {
        if text == "ready" {
            assert!(!self.panic_on_ready, "Intentional process callback panic");
            self.cancel |= self.cancel_on_ready;
        }
        if stream == 1 {
            self.output.push(text.into());
        } else {
            self.error.push(text.into());
        }
    }
    fn cancelled(&mut self) -> bool {
        self.cancel
    }
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "dvda-process-中文-日本語-🎵-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn job(args: &[&str]) -> Job {
    Job {
        file_name: env!("CARGO_BIN_EXE_dvda-cli").into(),
        arguments: ["process-fixture"]
            .iter()
            .chain(args.iter())
            .map(|s| (*s).into())
            .collect(),
        working_directory: None,
        environment: vec![],
        output_code_page: 65001,
        error_code_page: 65001,
        capture_output: true,
        capture_error: true,
        timeout_millis: Some(20000),
    }
}
fn passed(result: &Outcome, code: i32) {
    assert!(result.failure.is_none(), "{:?}", result.failure);
    assert_eq!(result.exit_code, Some(code));
}
#[test]
fn arguments_environment_and_streams() {
    let root = Scratch::new();
    let args = [
        "",
        "with spaces",
        "中文-日本語-한글-🎵",
        "\"",
        "a\\\"b",
        "\\",
        "ends in \\",
        "tab\tvalue",
        "line\nbreak",
    ];
    let mut request = job(&["echo"]);
    request.arguments.extend(args.map(String::from));
    request.working_directory = Some(root.0.to_str().unwrap().into());
    request.environment = vec![
        ("DVDA_PROCESS_VALUE".into(), Some("值=日本語🎵".into())),
        ("DVDA_PROCESS_REMOVE".into(), None),
    ];
    let mut events = Events::default();
    let result = execute(request, &mut events);
    passed(&result, 0);
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&events.output[0]).unwrap(),
        args
    );
    assert_eq!(
        events.error,
        [root.0.to_str().unwrap(), "值=日本語🎵", "missing"]
    );
    for code in [65001, 1200, 1201, 12000, 12001] {
        let mut request = job(&["lines", &code.to_string()]);
        request.output_code_page = code;
        request.error_code_page = code;
        let mut events = Events::default();
        let result = execute(request, &mut events);
        passed(&result, 0);
        assert_eq!(
            events.output,
            ["中文-日本語-🎵", "", "alpha", "bravo", "last\0tail"]
        );
        assert_eq!(events.error, ["error", "last"]);
        assert_eq!(
            result.standard_output,
            "中文-日本語-🎵\r\n\r\nalpha\r\nbravo\r\nlast\0tail\r\n"
        );
    }
    for capture in [true, false] {
        let mut request = job(&["burst"]);
        request.capture_output = capture;
        request.capture_error = capture;
        let mut events = Events::default();
        let result = execute(request, &mut events);
        passed(&result, 7);
        assert_eq!(events.output.len(), 2048);
        assert_eq!(events.error.len(), 2048);
        for i in 0..2048 {
            assert_eq!(events.output[i], format!("out-{i:05}{}", "x".repeat(80)));
            assert_eq!(events.error[i], format!("err-{i:05}{}", "y".repeat(80)));
        }
        assert_eq!(result.standard_output.is_empty(), !capture);
        assert_eq!(result.standard_error.is_empty(), !capture);
    }
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
    fn WaitForSingleObject(handle: *mut std::ffi::c_void, milliseconds: u32) -> u32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
}
fn assert_terminated(path: &std::path::Path) {
    let pid: u32 = fs::read_to_string(path)
        .expect("Fixture did not start")
        .parse()
        .unwrap();
    let handle = unsafe { OpenProcess(0x100000, 0, pid) };
    if !handle.is_null() {
        let code = unsafe { WaitForSingleObject(handle, 5000) };
        unsafe {
            CloseHandle(handle);
        }
        assert_eq!(code, 0, "Leaked descendant {pid}");
    } else {
        assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(87));
    }
}
#[test]
fn cancellation_timeout_panic_and_concurrency() {
    let root = Scratch::new();
    for mode in ["cancel", "timeout", "panic"] {
        let parent = root.0.join(format!("{mode}-parent.pid"));
        let child = root.0.join(format!("{mode}-child.pid"));
        let mut request = job(&["tree", parent.to_str().unwrap(), child.to_str().unwrap()]);
        if mode == "timeout" {
            request.timeout_millis = Some(1500);
        }
        let mut events = Events {
            cancel_on_ready: mode == "cancel",
            panic_on_ready: mode == "panic",
            ..Events::default()
        };
        let start = Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            execute(request, &mut events)
        }));
        if mode == "panic" {
            assert!(result.is_err());
        } else {
            assert_eq!(
                result.unwrap().failure.unwrap().kind,
                if mode == "cancel" {
                    "Cancelled"
                } else {
                    "Timeout"
                }
            );
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        assert_terminated(&parent);
        assert_terminated(&child);
    }
    let handles: Vec<_> = (0..4)
        .map(|i| {
            std::thread::spawn(move || {
                let mut events = Events::default();
                let result = execute(job(&["echo", &i.to_string()]), &mut events);
                passed(&result, 0);
                assert_eq!(
                    serde_json::from_str::<Vec<String>>(&events.output[0]).unwrap(),
                    [i.to_string()]
                );
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let mut missing = job(&["echo"]);
    missing.file_name = root.0.join("missing.exe").to_str().unwrap().into();
    assert_eq!(
        execute(missing, &mut Events::default())
            .failure
            .unwrap()
            .kind,
        "InvalidOperation"
    );
    let mut precancel = job(&["echo"]);
    precancel.working_directory = Some(root.0.join("absent").to_str().unwrap().into());
    assert_eq!(
        execute(
            precancel,
            &mut Events {
                cancel: true,
                ..Events::default()
            }
        )
        .failure
        .unwrap()
        .kind,
        "Cancelled"
    );
}
