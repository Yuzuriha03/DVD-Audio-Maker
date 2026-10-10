//! Menu XML validation and callback dispatch. Vendor callbacks are entered only
//! through a C setjmp island; a vendor exit must never jump over a Rust frame.
use quick_xml::{Reader, events::Event};
use std::{
    ffi::{CStr, CString, c_char, c_int},
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
};
#[cfg(feature = "direct-link")]
mod direct;
mod resources;
mod session;
#[cfg(feature = "direct-link")]
pub use direct::{
    MenuError, MenuRequest, ReadRgba, dvda_menu_run_navigation, dvda_menu_run_spu, run_navigation,
    run_spu,
};
#[cfg(all(feature = "direct-link", not(test)))]
use direct::{menu_accepts_body, menu_attribute, menu_body, menu_callback};

#[repr(C)]
pub struct Element {
    name: *const c_char,
    parent: c_int,
    state: c_int,
    start: Option<unsafe extern "C" fn()>,
    end: Option<unsafe extern "C" fn()>,
}
#[repr(C)]
pub struct Attribute {
    element: *const c_char,
    name: *const c_char,
    callback: Option<unsafe extern "C" fn(*const c_char)>,
}
#[cfg(all(not(test), not(feature = "direct-link")))]
unsafe extern "C" {
    fn menu_callback(callback: Option<unsafe extern "C" fn()>) -> c_int;
    fn menu_attribute(
        callback: Option<unsafe extern "C" fn(*const c_char)>,
        value: *const c_char,
    ) -> c_int;
    fn menu_body(value: *const c_char) -> c_int;
    fn menu_accepts_body() -> c_int;
}

#[cfg(test)]
unsafe fn menu_callback(callback: Option<unsafe extern "C" fn()>) -> c_int {
    if let Some(callback) = callback {
        unsafe { callback() };
    }
    0
}
#[cfg(test)]
unsafe fn menu_attribute(
    callback: Option<unsafe extern "C" fn(*const c_char)>,
    value: *const c_char,
) -> c_int {
    if let Some(callback) = callback {
        unsafe { callback(value) };
    }
    0
}
#[cfg(test)]
unsafe fn menu_body(_value: *const c_char) -> c_int {
    0
}
#[cfg(test)]
unsafe fn menu_accepts_body() -> c_int {
    0
}

#[derive(Debug, PartialEq)]
enum Token {
    Start(String, Vec<(String, String)>, bool),
    End(String),
    Text(String),
}

// Parse the entire document before invoking vendor callbacks. Malformed XML
// cannot partially configure vendor state; a failed transaction may still have
// created an output file, which the caller must discard.
fn xml_whitespace(text: &str) -> bool {
    text.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
}

fn xml_characters(text: &str) -> Result<(), String> {
    if text.chars().all(|c| matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)) {
        Ok(())
    } else {
        Err("invalid XML character".into())
    }
}

fn tokenize(input: &[u8]) -> Result<Vec<Token>, String> {
    let document = std::str::from_utf8(input).map_err(|e| e.to_string())?;
    xml_characters(document)?;
    let mut reader = Reader::from_reader(input);
    reader.config_mut().check_end_names = true;
    reader.config_mut().check_comments = true;
    let mut tokens = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut seen = false;
    let mut closed = false;
    let mut declaration = false;
    let mut preceding_event = false;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        let declaration_allowed = !preceding_event;
        preceding_event = true;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .map_err(|e| e.to_string())?
                    .to_owned();
                let mut attrs = Vec::new();
                for a in e.attributes() {
                    let a = a.map_err(|e| e.to_string())?;
                    let key = std::str::from_utf8(a.key.as_ref())
                        .map_err(|e| e.to_string())?
                        .to_owned();
                    let raw = std::str::from_utf8(a.value.as_ref()).map_err(|e| e.to_string())?;
                    let normalized = raw.replace("\r\n", " ").replace(['\r', '\n', '\t'], " ");
                    let value = quick_xml::escape::unescape(&normalized)
                        .map_err(|e| e.to_string())?
                        .into_owned();
                    xml_characters(&key)?;
                    xml_characters(&value)?;
                    attrs.push((key, value));
                }
                if closed || stack.len() >= 10 {
                    return Err("invalid root or excessive depth".into());
                }
                if stack.is_empty() {
                    if seen {
                        return Err("multiple roots".into());
                    }
                    seen = true;
                }
                if name.contains('\0') {
                    return Err("NUL in element".into());
                }
                tokens.push(Token::Start(name.clone(), attrs, empty));
                if empty {
                    if stack.is_empty() {
                        closed = true;
                    }
                } else {
                    stack.push(name);
                }
            }
            Event::End(e) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .map_err(|e| e.to_string())?
                    .to_owned();
                if stack.pop().as_deref() != Some(&name) {
                    return Err("unmatched end tag".into());
                }
                if stack.is_empty() {
                    closed = true;
                }
                tokens.push(Token::End(name));
            }
            Event::Text(e) => {
                let decoded = e.xml_content().map_err(|e| e.to_string())?;
                let text = quick_xml::escape::unescape(&decoded)
                    .map_err(|e| e.to_string())?
                    .into_owned();
                xml_characters(&text)?;
                if stack.is_empty() && !xml_whitespace(&text) {
                    return Err("text outside root".into());
                }
                tokens.push(Token::Text(text));
            }
            Event::CData(e) => {
                if stack.is_empty() {
                    return Err("CDATA outside root".into());
                }
                let value = e.xml_content().map_err(|e| e.to_string())?.into_owned();
                xml_characters(&value)?;
                tokens.push(Token::Text(value));
            }
            Event::GeneralRef(e) => {
                let reference = e.decode().map_err(|e| e.to_string())?;
                let value = quick_xml::escape::unescape(&format!("&{reference};"))
                    .map_err(|e| e.to_string())?
                    .into_owned();
                if stack.is_empty() {
                    return Err("reference outside root".into());
                }
                xml_characters(&value)?;
                tokens.push(Token::Text(value));
            }
            Event::Comment(_) => {}
            Event::Decl(e) if !seen && !declaration && declaration_allowed => {
                if e.version().map_err(|e| e.to_string())?.as_ref() != b"1.0" {
                    return Err("unsupported XML version".into());
                }
                if let Some(encoding) = e.encoding() {
                    let encoding = encoding.map_err(|e| e.to_string())?;
                    if !encoding.eq_ignore_ascii_case(b"utf-8") {
                        return Err("only UTF-8 XML is supported".into());
                    }
                }
                if let Some(standalone) = e.standalone() {
                    let standalone = standalone.map_err(|e| e.to_string())?;
                    if standalone.as_ref() != b"yes" && standalone.as_ref() != b"no" {
                        return Err("invalid XML standalone declaration".into());
                    }
                }
                declaration = true;
            }
            Event::Eof => break,
            _ => return Err("DTD, processing instruction or misplaced declaration".into()),
        }
    }
    if !seen || !closed || !stack.is_empty() {
        return Err("incomplete XML document".into());
    }
    Ok(tokens)
}

unsafe fn text<'a>(p: *const c_char) -> Result<&'a str, String> {
    if p.is_null() {
        return Err("null string".into());
    }
    unsafe { CStr::from_ptr(p) }
        .to_str()
        .map_err(|e| e.to_string())
}

unsafe fn dispatch(
    tokens: Vec<Token>,
    elements: *const Element,
    attributes: *const Attribute,
) -> Result<(), String> {
    if elements.is_null() || attributes.is_null() {
        return Err("null descriptor table".into());
    }
    let mut state = 0;
    let mut stack: Vec<usize> = Vec::new();
    let mut body = String::new();
    unsafe {
        if menu_body(std::ptr::null()) != 0 {
            return Err("body reset failed".into());
        }
    }
    for token in tokens {
        match token {
            Token::Start(name, attrs, empty) => {
                if !body.is_empty() {
                    return Err("mixed XML content".into());
                }
                let mut index = 0;
                loop {
                    let elem = unsafe { &*elements.add(index) };
                    if elem.name.is_null() {
                        return Err(format!("unsupported element {name}"));
                    }
                    if elem.parent == state && unsafe { text(elem.name)? } == name {
                        break;
                    }
                    index += 1;
                }
                let elem = unsafe { &*elements.add(index) };
                if unsafe { menu_callback(elem.start) } != 0 {
                    return Err("element callback failed".into());
                }
                for (key, value) in attrs {
                    let mut a = 0;
                    loop {
                        let attr = unsafe { &*attributes.add(a) };
                        if attr.element.is_null() {
                            return Err(format!("unsupported attribute {key}"));
                        }
                        if unsafe { text(attr.element)? } == name
                            && unsafe { text(attr.name)? } == key
                        {
                            break;
                        }
                        a += 1;
                    }
                    let value = CString::new(value).map_err(|e| e.to_string())?;
                    if unsafe { menu_attribute((*attributes.add(a)).callback, value.as_ptr()) } != 0
                    {
                        return Err("attribute callback failed".into());
                    }
                }
                if empty {
                    if unsafe { menu_callback(elem.end) } != 0 {
                        return Err("end callback failed".into());
                    }
                    unsafe {
                        if menu_body(std::ptr::null()) != 0 {
                            return Err("body reset failed".into());
                        }
                    }
                } else {
                    stack.push(index);
                    state = elem.state;
                }
            }
            Token::End(_) => {
                let index = stack.pop().ok_or("invalid callback stack")?;
                let elem = unsafe { &*elements.add(index) };
                if unsafe { menu_callback(elem.end) } != 0 {
                    return Err("end callback failed".into());
                }
                state = elem.parent;
                body.clear();
                unsafe {
                    if menu_body(std::ptr::null()) != 0 {
                        return Err("body reset failed".into());
                    }
                }
            }
            Token::Text(value) => {
                if unsafe { menu_accepts_body() } == 0 {
                    if !xml_whitespace(&value) {
                        return Err("unexpected XML text".into());
                    }
                } else {
                    body.push_str(&value);
                    let value = CString::new(body.as_str()).map_err(|e| e.to_string())?;
                    if unsafe { menu_body(value.as_ptr()) } != 0 {
                        return Err("body allocation failed".into());
                    }
                }
            }
        }
    }
    Ok(())
}

/// # Safety
/// Arguments must be valid terminated vendor descriptor tables and UTF-8 path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_readxml(
    path: *const c_char,
    elements: *const Element,
    attributes: *const Attribute,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), String> {
        let path = unsafe { text(path)? };
        let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        unsafe { dispatch(tokenize(&data)?, elements, attributes) }
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            // Broken/closed stderr must not panic outside the unwind guard.
            let _ = writeln!(std::io::stderr().lock(), "ERR: invalid menu XML: {e}");
            1
        }
        Err(_) => 1,
    }
}

/// # Safety
/// `value` must point to a terminated UTF-8 string, or be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn menu_rust_boolean(value: *const c_char) -> c_int {
    catch_unwind(|| {
        let Ok(value) = (unsafe { text(value) }) else {
            return -1;
        };
        match value.to_ascii_lowercase().as_str() {
            "1" | "on" | "yes" => 1,
            "0" | "off" | "no" => 0,
            _ => -1,
        }
    })
    .unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_malformed_and_dtd() {
        for xml in [
            "",
            "<a>",
            "<a></b>",
            "<a/><b/>",
            "text<a/>",
            "<a x='1' x='2'/>",
            "<!DOCTYPE a><a/>",
            "<a>&unknown;</a>",
            "<a>\0</a>",
            "<a>&#1;</a>",
            "<a x='&#65535;'/>",
            "<a><![CDATA[\u{b}]]></a>",
            "\u{a0}<a/>",
            "<!--comment--><?xml version='1.0'?><a/>",
            "<?xml version='1.0'?><?xml version='1.0'?><a/>",
            "<?xml version='1.1'?><a/>",
            "<?xml version='1.0' encoding='latin1'?><a/>",
            "<a><!--invalid--comment--></a>",
        ] {
            assert!(tokenize(xml.as_bytes()).is_err(), "{xml}");
        }
    }
    #[test]
    fn xml_attribute_normalization() {
        assert_eq!(
            tokenize(b"<a x='one\r\ntwo\tthree&#10;four'/>").unwrap(),
            vec![Token::Start(
                "a".into(),
                vec![("x".into(), "one two three\nfour".into())],
                true
            )]
        );
        assert!(tokenize(b"<?xml version='1.0' standalone='invalid'?><a/>").is_err());
    }
    #[test]
    fn utf8_entities_and_depth() {
        assert_eq!(
            tokenize("<菜单 路径='目录&amp;图像'>开始&#65;<![CDATA[结束]]></菜单>".as_bytes())
                .unwrap(),
            vec![
                Token::Start(
                    "菜单".into(),
                    vec![("路径".into(), "目录&图像".into())],
                    false
                ),
                Token::Text("开始".into()),
                Token::Text("A".into()),
                Token::Text("结束".into()),
                Token::End("菜单".into())
            ]
        );
        assert!(tokenize(format!("{}{}", "<a>".repeat(11), "</a>".repeat(11)).as_bytes()).is_err());
        assert!(tokenize(b"<a/><!---->").is_ok());
    }
}
