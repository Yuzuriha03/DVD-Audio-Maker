//! Structured, presentation-only workflow events. User paths/names are data.
use dvda_native::media::Callbacks;
use serde::{Deserialize, Serialize};

pub const PREFIX: &str = "[TASK] ";

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Event {
    pub action: String,
    pub current: u64,
    pub total: u64,
    pub name: String,
    pub detail: String,
    #[serde(default)]
    pub warning: bool,
}

pub fn emit(
    caller: &mut dyn Callbacks,
    action: &str,
    current: u64,
    total: u64,
    name: &str,
    detail: &str,
) {
    emit_event(
        caller,
        Event {
            action: action.into(),
            current,
            total,
            name: name.into(),
            detail: detail.into(),
            warning: false,
        },
    );
}

pub fn warning(caller: &mut dyn Callbacks, action: &str, count: u64, name: &str) {
    emit_event(
        caller,
        Event {
            action: action.into(),
            current: count,
            total: count,
            name: name.into(),
            detail: String::new(),
            warning: true,
        },
    );
}

fn emit_event(caller: &mut dyn Callbacks, event: Event) {
    caller.emit(
        if event.warning || event.action == "pcm_failed" {
            2
        } else {
            1
        },
        &format!("{PREFIX}{}", serde_json::to_string(&event).unwrap()),
    );
}

pub fn parse(text: &str) -> Option<Event> {
    serde_json::from_str(text.trim().strip_prefix(PREFIX)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opaque_names_paths_and_multiline_details_roundtrip() {
        struct Capture(String);
        impl Callbacks for Capture {
            fn emit(&mut self, _: i32, text: &str) {
                self.0 = text.into();
            }
            fn cancelled(&mut self) -> bool {
                false
            }
        }
        let mut events = Capture(String::new());
        emit(
            &mut events,
            "pcm_ok",
            5,
            147,
            "曲目 {0} \"日本語\"",
            "D:/album page 1/曲.flac\nsecond line",
        );
        let event = parse(&events.0).unwrap();
        assert_eq!(event.name, "曲目 {0} \"日本語\"");
        assert_eq!(event.detail, "D:/album page 1/曲.flac\nsecond line");
        assert!(!event.warning);
        warning(&mut events, "button_coordinates", 323, "disc 1");
        assert!(parse(&events.0).unwrap().warning);
    }
}
