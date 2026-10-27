//! Control-construction macros, shared by every module of the crate.

macro_rules! create {
    ($parent:expr, $class:expr, $title:expr, $style:expr, $x:expr, $y:expr, $width:expr, $height:expr, $id:expr $(,)?) => {
        create_from_spec(CreateSpec {
            parent: $parent,
            class: $class,
            title: $title,
            style: $style,
            x: $x,
            y: $y,
            width: $width,
            height: $height,
            id: $id,
        })
    };
}

macro_rules! label {
    ($parent:expr, $text:expr, $key:expr, $x:expr, $y:expr, $w:expr, $h:expr, $labels:expr $(,)?) => {{
        let hwnd = create!(
            $parent,
            "STATIC",
            $text,
            WS_CHILD | WS_VISIBLE,
            $x,
            $y,
            $w,
            $h,
            0
        );
        if !$key.is_empty() {
            $labels.push((hwnd, $key));
        }
        hwnd
    }};
}

macro_rules! text_field {
    ($parent:expr, $caption:expr, $key:expr, $id:expr, $x:expr, $y:expr, $width:expr, $labels:expr $(,)?) => {{
        label!($parent, $caption, $key, $x, $y, $width, 20, $labels);
        create!(
            $parent,
            "EDIT",
            "",
            WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL | WS_TABSTOP,
            $x,
            $y + 20,
            $width,
            25,
            $id
        )
    }};
}

macro_rules! browse_edit {
    ($parent:expr, $caption:expr, $key:expr, $id:expr, $browse_id:expr, $x:expr, $y:expr, $labels:expr, $browse:expr, $browse_buttons:expr, $lang:expr $(,)?) => {{
        // Room for the caption's line above a field 20 DIPs lower.
        label!($parent, $caption, $key, $x, $y - 4, 520, 22, $labels);
        let edit = create!(
            $parent,
            "EDIT",
            "",
            WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL | WS_TABSTOP,
            $x,
            $y + 20,
            820,
            24,
            $id
        );
        // End before the next row's label (24 logical pixels below the edit).
        let browse_button = button(
            $parent,
            tr($lang, "browse"),
            $browse_id,
            $x + 830,
            $y + 20,
            90,
            24,
        );
        $browse_buttons.push(browse_button);
        $browse.insert(
            $browse_id,
            (edit, BrowseKind::Folder),
        );
        edit
    }};
}

macro_rules! combo_field {
    ($parent:expr, $caption:expr, $key:expr, $id:expr, $x:expr, $y:expr, $width:expr, $labels:expr $(,)?) => {{
        label!($parent, $caption, $key, $x, $y, $width, 20, $labels);
        combo($parent, $id, $x, $y + 20, $width, 160)
    }};
}
