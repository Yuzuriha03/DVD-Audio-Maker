//! Menu XML tokenizer shared by the SPU and navigation implementations.
#[cfg(any(feature = "direct-link", test))]
use quick_xml::{Reader, XmlVersion, events::Event};

#[cfg(feature = "direct-link")]
mod direct;
#[cfg(any(feature = "direct-link", test))]
mod nav;
#[cfg(any(feature = "direct-link", test))]
mod resources;
#[cfg(feature = "direct-link")]
mod session;
#[cfg(any(feature = "direct-link", test))]
mod spu;
#[cfg(feature = "direct-link")]
pub use direct::{
    MenuError, MenuRequest, ReadRgba, dvda_menu_run_navigation, dvda_menu_run_spu, run_navigation,
    run_spu, run_spu_files,
};

#[cfg(any(feature = "direct-link", test))]
#[derive(Debug, PartialEq)]
pub(crate) enum Token {
    Start(String, Vec<(String, String)>, bool),
    End(String),
    Text(String),
}

// Parse the entire document before invoking vendor callbacks. Malformed XML
// cannot partially configure vendor state; a failed transaction may still have
// created an output file, which the caller must discard.
#[cfg(any(feature = "direct-link", test))]
fn xml_whitespace(text: &str) -> bool {
    text.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
}

#[cfg(any(feature = "direct-link", test))]
fn xml_characters(text: &str) -> Result<(), String> {
    if text.chars().all(|c| matches!(c as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x0010_FFFF)) {
        Ok(())
    } else {
        Err("invalid XML character".into())
    }
}

#[cfg(any(feature = "direct-link", test))]
#[expect(
    clippy::too_many_lines,
    reason = "One ordered event loop preserves XML declaration, depth, and error precedence"
)]
pub(crate) fn tokenize(input: &[u8]) -> Result<Vec<Token>, String> {
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
    let mut xml_version = XmlVersion::Implicit1_0;
    let mut preceding_event = false;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        let declaration_allowed = !preceding_event;
        preceding_event = true;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let name = e.name().as_ref().to_owned();
                let mut attrs = Vec::new();
                for a in e.attributes() {
                    let a = a.map_err(|e| e.to_string())?;
                    let key = a.key.as_ref().to_owned();
                    let raw = a.value.as_ref();
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
                let name = e.name().as_ref().to_owned();
                if stack.pop().as_deref() != Some(&name) {
                    return Err("unmatched end tag".into());
                }
                if stack.is_empty() {
                    closed = true;
                }
                tokens.push(Token::End(name));
            }
            Event::Text(e) => {
                let decoded = e.xml_content(xml_version);
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
                let value = e.xml_content(xml_version).into_owned();
                xml_characters(&value)?;
                tokens.push(Token::Text(value));
            }
            Event::GeneralRef(e) => {
                let reference = e.into_inner();
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
                if e.version().map_err(|e| e.to_string())?.as_ref() != "1.0" {
                    return Err("unsupported XML version".into());
                }
                if let Some(encoding) = e.encoding() {
                    let encoding = encoding.map_err(|e| e.to_string())?;
                    if !encoding.eq_ignore_ascii_case("utf-8") {
                        return Err("only UTF-8 XML is supported".into());
                    }
                }
                if let Some(standalone) = e.standalone() {
                    let standalone = standalone.map_err(|e| e.to_string())?;
                    if standalone.as_ref() != "yes" && standalone.as_ref() != "no" {
                        return Err("invalid XML standalone declaration".into());
                    }
                }
                declaration = true;
                xml_version = XmlVersion::Explicit1_0;
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
