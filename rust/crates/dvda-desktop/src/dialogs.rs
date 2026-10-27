//! Folder and file dialogs, the output-folder shortcut and message boxes.

use super::*;

pub(crate) fn open_output(hwnd: Hwnd, state: &UiState) {
    let path = get_text(state.controls.final_dir);
    if !Path::new(&path).is_dir() {
        message_box(hwnd, tr(state.lang(), "no_output"), "DVD-Audio Maker", 0x10);
        return;
    }
    let path = wide(&path);
    let verb = wide("open");
    unsafe {
        ShellExecuteW(
            hwnd,
            verb.as_ptr(),
            path.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOW,
        );
    }
}

pub(crate) fn browse_target(hwnd: Hwnd, state: &mut UiState, id: usize) {
    let Some((target, kind)) = state.browse.get(&id).copied() else {
        return;
    };
    let current = get_text(target);
    let result = match kind {
        BrowseKind::Folder => folder_dialog(hwnd, &current, tr(state.lang(), "browse")),
        BrowseKind::Font => file_dialog(
            hwnd,
            false,
            &current,
            tr(state.lang(), "browse"),
            state.lang(),
            FileKind::Font,
        ),

    };
    if let Some(path) = result {
        set_text(target, &path);
    }
}

pub(crate) fn folder_dialog(owner: Hwnd, current: &str, title: &str) -> Option<String> {
    let mut display = vec![0u16; 32768];
    let title = wide(title);
    let info = BrowseInfo {
        owner,
        root: ptr::null_mut(),
        display_name: display.as_mut_ptr(),
        title: title.as_ptr(),
        flags: 0x0041,
        callback: None,
        param: 0,
        image: 0,
    };
    let item = unsafe { SHBrowseForFolderW(&info) };
    if item.is_null() {
        return None;
    }
    let mut path = vec![0u16; 32768];
    let ok = unsafe { SHGetPathFromIDListW(item, path.as_mut_ptr()) };
    unsafe {
        CoTaskMemFree(item);
    }
    if ok == 0 {
        return None;
    }
    let value =
        String::from_utf16_lossy(&path[..path.iter().position(|v| *v == 0).unwrap_or(path.len())]);
    if value.is_empty() && !current.is_empty() {
        Some(current.to_owned())
    } else {
        Some(value)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum FileKind {
    Profile,
    Log,
    Font,
}

pub(crate) fn dialog_filter(lang: Lang, kind: FileKind) -> (String, &'static str) {
    let all = format!("{}\0*.*\0\0", tr(lang, "all_filter"));
    match kind {
        FileKind::Profile => (
            format!("{}\0*.json\0{all}", tr(lang, "json_filter")),
            "json",
        ),
        FileKind::Log => (
            format!("{}\0*.txt;*.log\0{all}", tr(lang, "text_filter")),
            "txt",
        ),
        FileKind::Font => (
            format!("{}\0*.ttf;*.otf;*.ttc\0{all}", tr(lang, "font_filter")),
            "",
        ),
    }
}

pub(crate) fn file_dialog(
    owner: Hwnd,
    save: bool,
    current: &str,
    title: &str,
    lang: Lang,
    kind: FileKind,
) -> Option<String> {
    let (filter, extension) = dialog_filter(lang, kind);
    let filter = wide(&filter);
    let title = wide(title);
    let default_extension = wide(extension);
    let mut file = vec![0u16; 32768];
    for (i, unit) in current.encode_utf16().enumerate().take(file.len() - 1) {
        file[i] = unit;
    }
    let mut dialog = OpenFileName {
        size: std::mem::size_of::<OpenFileName>() as u32,
        owner,
        instance: ptr::null_mut(),
        filter: filter.as_ptr(),
        custom_filter: ptr::null_mut(),
        custom_filter_max: 0,
        filter_index: 1,
        file: file.as_mut_ptr(),
        file_max: file.len() as u32,
        file_title: ptr::null_mut(),
        file_title_max: 0,
        initial_dir: ptr::null(),
        title: title.as_ptr(),
        flags: 0x00000800 | if save { 0x00000002 } else { 0x00001000 },
        file_offset: 0,
        file_extension: 0,
        def_ext: default_extension.as_ptr(),
        data: 0,
        hook: ptr::null_mut(),
        template_name: ptr::null(),
        reserved: ptr::null_mut(),
        reserved2: 0,
        flags_ex: 0,
    };
    let ok = unsafe {
        if save {
            GetSaveFileNameW(&mut dialog)
        } else {
            GetOpenFileNameW(&mut dialog)
        }
    };
    if ok == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(
        &file[..file.iter().position(|v| *v == 0).unwrap_or(file.len())],
    ))
}

pub(crate) fn message_box(hwnd: Hwnd, text: &str, caption: &str, flags: u32) -> i32 {
    let text = wide(text);
    let caption = wide(caption);
    unsafe { MessageBoxW(hwnd, text.as_ptr(), caption.as_ptr(), flags) }
}
