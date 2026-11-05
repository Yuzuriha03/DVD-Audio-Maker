//! Subpicture image loading, palette grouping, button regions and cropping.
//!
//! Port of the spumux image stage used by the menu authoring path. Iteration
//! order and integer arithmetic follow the C implementation so the encoded
//! subpicture bytes stay identical.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::too_many_lines,
    reason = "Subpicture coordinates, palette indices, and crop arithmetic intentionally mirror the bounded C implementation"
)]
use super::color::Color;
use std::path::{Path, PathBuf};

pub(crate) const MAXX: usize = 720;
pub(crate) const MAXY: usize = 576;

/// Reads RGBA pixels into the supplied buffer (row-major, four bytes per pixel)
/// and returns the image dimensions. Mirrors the C `DvdaReadRgba` callback.
pub(crate) type RgbaReader<'a> = dyn FnMut(&Path, &mut [u8]) -> Result<(u32, u32), String> + 'a;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x0: i32,
    pub(crate) y0: i32,
    pub(crate) x1: i32,
    pub(crate) y1: i32,
}

impl Rect {
    pub(crate) const UNBOUND: Self = Self {
        x0: -1,
        y0: -1,
        x1: -1,
        y1: -1,
    };
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Button {
    pub(crate) name: Option<String>,
    pub(crate) autoaction: bool,
    pub(crate) r: Rect,
    pub(crate) up: Option<String>,
    pub(crate) down: Option<String>,
    pub(crate) left: Option<String>,
    pub(crate) right: Option<String>,
    pub(crate) group: i32,
}

impl Button {
    pub(crate) fn new(autoaction: bool) -> Self {
        Self {
            autoaction,
            r: Rect::UNBOUND,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Pict {
    pub(crate) fname: Option<PathBuf>,
    pub(crate) img: Vec<u8>,
    pub(crate) pal: Vec<Color>,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

impl Pict {
    fn construct_blank(&mut self, width: usize, height: usize) {
        self.width = width + (width & 1);
        self.height = height + (height & 1);
        self.fname = None;
        self.img = vec![0; self.width * self.height];
        self.pal = vec![Color::TRANSPARENT];
    }

    fn create_image(&mut self, width: usize, height: usize) {
        self.pal.clear();
        self.width = width + (width & 1);
        self.height = height + (height & 1);
        self.img = vec![0; self.width * self.height];
        if width != self.width {
            for y in 0..height {
                self.put_pixel(width + y * self.width, Color::TRANSPARENT);
            }
        }
        if height != self.height {
            for x in 0..self.width {
                self.put_pixel(x + height * self.width, Color::TRANSPARENT);
            }
        }
    }

    fn put_pixel(&mut self, at: usize, color: Color) {
        let color = if color.a == 0 && (color.r != 0 || color.g != 0 || color.b != 0) {
            Color::TRANSPARENT
        } else {
            color
        };
        if let Some(index) = self.pal.iter().position(|entry| *entry == color) {
            self.img[at] = index as u8;
            return;
        }
        if self.pal.len() == 256 {
            self.img[at] = 0;
            return;
        }
        self.img[at] = self.pal.len() as u8;
        self.pal.push(color);
    }

    fn read(&mut self, rgba: &mut RgbaReader<'_>) -> Result<(), String> {
        let Some(fname) = self.fname.clone() else {
            return Ok(());
        };
        let mut buffer = vec![0u8; MAXX * MAXY * 4];
        let (width, height) = rgba(&fname, &mut buffer)?;
        let (width, height) = (width as usize, height as usize);
        if width == 0 || height == 0 || width > MAXX || height > MAXY {
            return Err(format!(
                "bad image {}: {}x{}",
                fname.display(),
                width,
                height
            ));
        }
        self.create_image(width, height);
        for y in 0..height {
            for x in 0..width {
                let at = (y * width + x) * 4;
                let color = Color {
                    r: buffer[at],
                    g: buffer[at + 1],
                    b: buffer[at + 2],
                    a: buffer[at + 3],
                };
                self.put_pixel(y * self.width + x, color);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SubPicture {
    pub(crate) x0: u32,
    pub(crate) y0: u32,
    pub(crate) xd: u32,
    pub(crate) yd: u32,
    pub(crate) spts: i32,
    pub(crate) sd: i32,
    pub(crate) forced: bool,
    pub(crate) numpal: usize,
    pub(crate) autooutline: bool,
    pub(crate) autoorder: bool,
    pub(crate) outlinewidth: u32,
    pub(crate) img: Pict,
    pub(crate) hlt: Pict,
    pub(crate) sel: Pict,
    pub(crate) fimg: Vec<u8>,
    pub(crate) pal: [Color; 4],
    pub(crate) masterpal: [Color; 16],
    pub(crate) transparentc: Color,
    pub(crate) numgroups: usize,
    pub(crate) groupmap: [[i32; 4]; 3],
    pub(crate) buttons: Vec<Button>,
}

impl SubPicture {
    pub(crate) fn new() -> Self {
        Self {
            pal: [Color::TRANSPARENT; 4],
            transparentc: Color::TRANSPARENT,
            ..Default::default()
        }
    }

    pub(crate) fn find_master_pal(&mut self, color: Color) -> i32 {
        if color.a == 0 {
            return 0;
        }
        for (index, entry) in self.masterpal[..self.numpal].iter().enumerate() {
            if entry.r == color.r && entry.g == color.g && entry.b == color.b {
                return index as i32;
            }
        }
        assert!(self.numpal < 16, "master palette overflow");
        self.masterpal[self.numpal] = color;
        self.numpal += 1;
        (self.numpal - 1) as i32
    }

    fn scan_pict(&mut self, pict: &mut Pict) {
        // Replace the requested transparent colour only when the image carries no
        // alpha information of its own.
        if pict.pal.iter().all(|entry| entry.a == 255)
            && let Some(entry) = pict
                .pal
                .iter_mut()
                .find(|entry| **entry == self.transparentc)
        {
            *entry = Color::TRANSPARENT;
        }
        for entry in pict.pal.clone() {
            self.find_master_pal(entry);
        }
    }

    pub(crate) fn process(&mut self, rgba: &mut RgbaReader<'_>) -> Result<(), String> {
        let mut img = std::mem::take(&mut self.img);
        img.read(rgba)?;
        self.scan_pict(&mut img);
        self.img = img;
        let mut hlt = std::mem::take(&mut self.hlt);
        hlt.read(rgba)?;
        self.scan_pict(&mut hlt);
        self.hlt = hlt;
        let mut sel = std::mem::take(&mut self.sel);
        sel.read(rgba)?;
        self.scan_pict(&mut sel);
        self.sel = sel;

        let mut width = 0usize;
        let mut height = 0usize;
        for pict in [&self.img, &self.hlt, &self.sel] {
            if !pict.img.is_empty() {
                width = pict.width;
                height = pict.height;
                break;
            }
        }
        if width == 0 {
            return Err("no picture".into());
        }
        for (pict, name) in [
            (&mut self.img, "img"),
            (&mut self.hlt, "hlt"),
            (&mut self.sel, "sel"),
        ] {
            if pict.img.is_empty() {
                pict.construct_blank(width, height);
            } else if pict.width != width || pict.height != height {
                return Err(format!("inconsistent picture dimensions for {name}"));
            }
        }
        self.crop_and_group()
    }

    fn crop_and_group(&mut self) -> Result<(), String> {
        if self.x0 >= MAXX as u32 || self.y0 >= MAXY as u32 {
            return Err("subpicture offset is outside the DVD frame".into());
        }
        let w = self.img.width;
        let h = self.img.height;
        self.xd = w as u32;
        self.yd = h as u32;

        if self.autooutline {
            self.detect_buttons();
        }
        if self.buttons.len() > 36 {
            return Err("a DVD subpicture supports at most 36 buttons".into());
        }
        self.fix_names();
        self.detect_directions();

        self.fimg = vec![255; w * h];

        let mut useimg = true;
        if !self.buttons.is_empty() {
            loop {
                if self.pick_button_groups(1, useimg)
                    || self.pick_button_groups(2, useimg)
                    || self.pick_button_groups(3, useimg)
                {
                    break;
                }
                if !useimg {
                    return Err("cannot pick button masks".into());
                }
                useimg = false;
            }
            self.fill_button_areas(useimg)?;
        }

        self.pal.fill(Color {
            r: 255,
            g: 255,
            b: 255,
            a: 0,
        });
        for index in 0..(w * h) {
            let slot = self.fimg[index];
            if slot != 255 {
                self.pal[slot as usize] = self.img.pal[self.img.img[index] as usize];
            }
        }
        for index in 0..(w * h) {
            if self.fimg[index] != 255 {
                continue;
            }
            let color = self.img.pal[self.img.img[index] as usize];
            let mut found = None;
            for (slot, entry) in self.pal.iter().enumerate() {
                if *entry == color {
                    found = Some(slot);
                    break;
                }
            }
            if found.is_none() {
                for (slot, entry) in self.pal.iter().enumerate() {
                    if entry.a == 0 && entry.r == 255 {
                        self.pal[slot] = color;
                        found = Some(slot);
                        break;
                    }
                }
            }
            let slot = found.ok_or_else(|| "too many colors in base picture".to_string())?;
            self.fimg[index] = slot as u8;
        }

        // Minimal visual area, then crop the image and adjust the button rectangles.
        let mut x0 = w;
        let mut first_y0: i32 = -1;
        let mut xd = 0usize;
        let mut yd = 0usize;
        for y in 0..h {
            for x in 0..w {
                let index = y * w + x;
                if self.img.pal[self.img.img[index] as usize].a != 0
                    || self.hlt.pal[self.hlt.img[index] as usize].a != 0
                    || self.sel.pal[self.sel.img[index] as usize].a != 0
                {
                    if first_y0 == -1 {
                        first_y0 = y as i32;
                    }
                    yd = y;
                    if x < x0 {
                        x0 = x;
                    }
                    if x > xd {
                        xd = x;
                    }
                }
            }
        }
        if first_y0 == -1 {
            self.xd = w as u32;
            self.yd = h as u32;
            return Ok(());
        }
        let crop_x0 = (x0 as i32) & -2;
        let crop_y0 = first_y0 & -2;
        self.xd = ((xd as i32 + 2 - crop_x0) & -2) as u32;
        self.yd = ((yd as i32 + 2 - crop_y0) & -2) as u32;
        let xd_usize = self.xd as usize;
        let yd_usize = self.yd as usize;
        for row in 0..yd_usize {
            let source = row * w + crop_x0 as usize + crop_y0 as usize * w;
            self.fimg
                .copy_within(source..source + xd_usize, row * xd_usize);
        }
        for button in &mut self.buttons {
            button.r.x0 += self.x0 as i32;
            button.r.y0 += self.y0 as i32;
            button.r.x1 += self.x0 as i32;
            button.r.y1 += self.y0 as i32;
        }
        self.x0 = (self.x0 as i32 + crop_x0) as u32;
        self.y0 = (self.y0 as i32 + crop_y0) as u32;
        Ok(())
    }

    fn fill_button_areas(&mut self, useimg: bool) -> Result<(), String> {
        let w = self.img.width;
        for index in 0..self.buttons.len() {
            let group = {
                let button = &self.buttons[index];
                if button.group <= 0 {
                    return Err(format!("button {index} has no group"));
                }
                (button.group - 1) as usize
            };
            let rect = self.buttons[index].r;
            for y in rect.y0..rect.y1 {
                for x in rect.x0..rect.x1 {
                    let at = (y * w as i32 + x) as usize;
                    let color = self.tricolor(at, useimg);
                    let slot = self.groupmap[group]
                        .iter()
                        .position(|entry| *entry == color)
                        .ok_or_else(|| {
                            format!("button {index} cannot find color {color:06x} in group {group}")
                        })?;
                    if self.fimg[at] != 255 && self.fimg[at] != slot as u8 {
                        return Err("overlapping buttons".into());
                    }
                    self.fimg[at] = slot as u8;
                }
            }
        }
        Ok(())
    }

    fn tricolor(&self, at: usize, useimg: bool) -> i32 {
        (if useimg {
            i32::from(self.img.img[at]) << 16
        } else {
            0
        }) | i32::from(self.hlt.img[at]) << 8
            | i32::from(self.sel.img[at])
    }

    fn fix_names(&mut self) {
        for (index, button) in self.buttons.iter_mut().enumerate() {
            if button.name.is_none() {
                button.name = Some((index + 1).to_string());
            }
        }
    }

    fn detect_directions(&mut self) {
        let mut results = Vec::with_capacity(self.buttons.len());
        for index in 0..self.buttons.len() {
            let button = self.buttons[index].clone();
            let mut found = [
                button.up.clone(),
                button.down.clone(),
                button.left.clone(),
                button.right.clone(),
            ];
            for (slot, angle) in found.iter_mut().zip([0i32, 180, 270, 90]) {
                if slot.is_none() {
                    *slot = Some(self.find_best_bindir(index, angle));
                }
            }
            results.push(found);
        }
        for (button, found) in self.buttons.iter_mut().zip(results) {
            let [up, down, left, right] = found;
            button.up = up;
            button.down = down;
            button.left = left;
            button.right = right;
        }
    }

    fn find_best_bindir(&self, source: usize, direction: i32) -> String {
        let button = &self.buttons[source];
        let mut best: Option<(String, i32, i32)> = None;
        for (index, candidate) in self.buttons.iter().enumerate() {
            if index == source {
                continue;
            }
            let Some((angle, distance)) = button_relative(&candidate.r, &button.r) else {
                continue;
            };
            let error = (angle - direction).abs();
            if error >= 90 {
                continue;
            }
            let better = match &best {
                None => true,
                Some((_, best_error, best_distance)) => {
                    error < *best_error || (error == *best_error && distance < *best_distance)
                }
            };
            if better {
                best = Some((candidate.name.clone().unwrap_or_default(), error, distance));
            }
        }
        match best {
            Some((name, _, _)) => name,
            None => button.name.clone().unwrap_or_default(),
        }
    }

    fn pick_button_groups(&mut self, groups: usize, useimg: bool) -> bool {
        let buttons = self.buttons.len();
        let mut button_palettes = vec![[0i32; 4]; buttons];
        let mut button_counts = vec![0usize; buttons];
        for index in 0..buttons {
            let rect = self.buttons[index].r;
            if rect.x0 == rect.x1 && rect.y0 == rect.y1 {
                continue;
            }
            if rect.x0 != rect.x1
                && rect.y0 != rect.y1
                && (rect.x0 < 0
                    || rect.x0 > rect.x1
                    || rect.x1 > self.xd as i32
                    || rect.y0 < 0
                    || rect.y0 > rect.y1
                    || rect.y1 > self.yd as i32)
            {
                return false;
            }
            let mut palette = button_palettes[index];
            let mut count = button_counts[index];
            for y in rect.y0..rect.y1 {
                for x in rect.x0..rect.x1 {
                    let color = self.tricolor((y as usize) * self.xd as usize + x as usize, useimg);
                    if !check_color(&mut palette, &mut count, color) {
                        return false;
                    }
                }
            }
            button_palettes[index] = palette;
            button_counts[index] = count;
        }

        let mut group_palettes = vec![[0i32; 4]; groups];
        let mut group_counts = vec![0usize; groups];
        let combinations = groups.pow(buttons as u32);
        for combination in 0..combinations {
            group_counts.fill(0);
            let mut remaining = combination;
            let mut ok = true;
            for index in 0..buttons {
                let target = remaining % groups;
                self.buttons[index].group = (target + 1) as i32;
                remaining /= groups;
                for entry in &button_palettes[index][..button_counts[index]] {
                    if !check_color(
                        &mut group_palettes[target],
                        &mut group_counts[target],
                        *entry,
                    ) {
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            if !ok {
                continue;
            }

            let mut groupmap = [[-1i32; 4]; 3];
            if useimg {
                let mut palette = [0i32; 4];
                let mut palette_count = self.img.pal.len();
                for (slot, entry) in palette.iter_mut().enumerate().take(palette_count) {
                    *entry = slot as i32;
                }
                'groups: for group in 0..groups {
                    groupmap[group].fill(-1);
                    for entry in &mut palette[..palette_count] {
                        *entry &= 255;
                    }
                    for value in &group_palettes[group][..group_counts[group]] {
                        let color = value >> 16;
                        let mut found = None;
                        for (slot, entry) in palette.iter().enumerate().take(palette_count) {
                            if *entry == color {
                                found = Some(slot);
                                break;
                            }
                        }
                        let slot = if let Some(slot) = found {
                            slot
                        } else {
                            if palette_count == 4 {
                                ok = false;
                                break 'groups;
                            }
                            palette_count += 1;
                            palette_count - 1
                        };
                        palette[slot] = color | 256;
                        groupmap[group][slot] = *value;
                    }
                }
                if !ok {
                    continue;
                }
            } else {
                for group in 0..groups {
                    for slot in 0..4 {
                        groupmap[group][slot] = if slot < group_counts[group] {
                            group_palettes[group][slot]
                        } else {
                            -1
                        };
                    }
                }
            }

            // Prefer a fully transparent entry at slot 0.
            for map in groupmap.iter_mut().take(groups) {
                let mut spare: Option<usize> = None;
                let mut tri = 0i32;
                for (slot, &entry) in map.iter().enumerate() {
                    tri = entry;
                    if tri == -1
                        || (self.img.pal[((tri >> 16) & 0xFF) as usize].a == 0
                            && self.hlt.pal[((tri >> 8) & 0xFF) as usize].a == 0
                            && self.sel.pal[(tri & 0xFF) as usize].a == 0)
                    {
                        spare = Some(slot);
                        break;
                    }
                }
                let Some(spare) = spare else {
                    continue;
                };
                if spare > 0 && tri == -1 {
                    tri = 0;
                    if let Some(index) = self.img.pal.iter().position(|entry| entry.a == 0) {
                        tri |= (index as i32) << 16;
                    }
                    if let Some(index) = self.hlt.pal.iter().position(|entry| entry.a == 0) {
                        tri |= (index as i32) << 8;
                    }
                    if let Some(index) = self.sel.pal.iter().position(|entry| entry.a == 0) {
                        tri |= index as i32;
                    }
                }
                if spare > 0 {
                    map[spare] = map[0];
                    map[0] = tri;
                }
            }

            self.groupmap = groupmap;
            self.numgroups = groups;
            return true;
        }
        false
    }

    fn detect_buttons(&mut self) {
        let w = self.xd as usize;
        let h = self.yd as usize;
        let mut visit = vec![0u8; w * h];
        if self.outlinewidth == 0 {
            self.outlinewidth = 1;
        }
        for (index, visited) in visit.iter_mut().enumerate() {
            *visited = u8::from(
                self.hlt.pal[self.hlt.img[index] as usize].a != 0
                    || self.sel.pal[self.sel.img[index] as usize].a != 0,
            );
        }
        let mut rects: Vec<Rect> = Vec::new();
        for y in 0..h {
            for x in 0..w {
                if visit[y * w + x] == 0 {
                    continue;
                }
                let mut rect = Rect {
                    x0: x as i32,
                    y0: y as i32,
                    x1: x as i32 + 1,
                    y1: y as i32 + 1,
                };
                loop {
                    let mut worked = false;
                    while self.scan_vline(&visit, &rect, rect.x0, -1) {
                        rect.x0 -= 1;
                        worked = true;
                    }
                    while self.scan_vline(&visit, &rect, rect.x1 - 1, 1) {
                        rect.x1 += 1;
                        worked = true;
                    }
                    while self.scan_hline(&visit, &rect, rect.y0, -1) {
                        rect.y0 -= 1;
                        worked = true;
                    }
                    while self.scan_hline(&visit, &rect, rect.y1 - 1, 1) {
                        rect.y1 += 1;
                        worked = true;
                    }
                    if !worked {
                        break;
                    }
                }
                rect.y0 -= rect.y0 & 1;
                rect.y1 += rect.y1 & 1;
                rects.push(rect);
                for row in rect.y0..rect.y1 {
                    for column in rect.x0..rect.x1 {
                        visit[(row as usize) * w + column as usize] = 0;
                    }
                }
            }
        }

        while !rects.is_empty() {
            let mut chosen = 0usize;
            if self.autoorder {
                for index in 1..rects.len() {
                    if rects[index].x0 < rects[chosen].x0
                        || (rects[index].x0 == rects[chosen].x0
                            && rects[index].y0 < rects[chosen].y0)
                    {
                        chosen = index;
                    }
                }
            } else {
                for index in 1..rects.len() {
                    if rects[index].y0 < rects[chosen].y0
                        || (rects[index].y0 == rects[chosen].y0
                            && rects[index].x0 < rects[chosen].x0)
                    {
                        chosen = index;
                    }
                }
            }
            for index in 0..rects.len() {
                if index == chosen {
                    continue;
                }
                if let Some((angle, _)) = button_relative(&rects[index], &rects[chosen]) {
                    let wanted = if self.autoorder { 0 } else { 270 };
                    if angle == wanted {
                        chosen = index;
                    }
                }
            }
            let slot = self.buttons.iter().position(|button| button.r.x0 < 0);
            let index = if let Some(index) = slot {
                index
            } else {
                self.buttons.push(Button::new(false));
                self.buttons.len() - 1
            };
            self.buttons[index].r = rects[chosen];
            rects.remove(chosen);
        }
    }

    fn scan_vline(&self, visit: &[u8], rect: &Rect, x: i32, direction: i32) -> bool {
        let w = self.xd as i32;
        let h = self.yd as i32;
        let mut x = x;
        for _ in 1..=self.outlinewidth {
            x += direction;
            if x < 0 || x >= w {
                return false;
            }
            for y in rect.y0.max(0)..rect.y1.min(h) {
                if visit[(y * w + x) as usize] != 0 {
                    return true;
                }
            }
        }
        false
    }

    fn scan_hline(&self, visit: &[u8], rect: &Rect, y: i32, direction: i32) -> bool {
        let w = self.xd as i32;
        let h = self.yd as i32;
        let mut y = y;
        for _ in 1..=self.outlinewidth {
            y += direction;
            if y < 0 || y >= h {
                return false;
            }
            for x in rect.x0.max(0)..rect.x1.min(w) {
                if visit[(y * w + x) as usize] != 0 {
                    return true;
                }
            }
        }
        false
    }
}

fn check_color(palette: &mut [i32; 4], count: &mut usize, color: i32) -> bool {
    if palette[..*count].contains(&color) {
        return true;
    }
    if *count == 4 {
        return false;
    }
    palette[*count] = color;
    *count += 1;
    true
}

fn angle_between(a0: i32, a1: i32, b0: i32, b1: i32, distance: i32) -> (i32, i32) {
    if a1 > b0 && a0 < b1 {
        return (0, distance);
    }
    let mut d1 = -(b0 - a1 + 1);
    let d2 = a0 - b1 + 1;
    let distance = distance + 1;
    if d2.abs() < d1.abs() {
        d1 = d2;
    }
    let mut sign = 1;
    if d1 < 0 {
        d1 = -d1;
        sign = -1;
    }
    let angle = f64::from(sign) * 180.0 / std::f64::consts::PI
        * (f64::from(d1) / f64::from(distance)).atan();
    let dist = f64::from(d1 * d1 + distance * distance).sqrt();
    (angle as i32, dist as i32)
}

fn button_relative(a: &Rect, b: &Rect) -> Option<(i32, i32)> {
    if a.y1 <= b.y0 {
        return Some(angle_between(a.x0, a.x1, b.x0, b.x1, b.y0 - a.y1));
    }
    if a.y0 >= b.y1 {
        let (mut angle, distance) = angle_between(a.x0, a.x1, b.x0, b.x1, a.y0 - b.y1);
        angle = 180 - angle;
        return Some((angle, distance));
    }
    if a.x1 <= b.x0 {
        let (angle, distance) = angle_between(a.y0, a.y1, b.y0, b.y1, b.x0 - a.x1);
        return Some((270 - angle, distance));
    }
    if a.x0 >= b.x1 {
        let (angle, distance) = angle_between(a.y0, a.y1, b.y0, b.y1, a.x0 - b.x1);
        return Some((90 + angle, distance));
    }
    None
}
