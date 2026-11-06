use dvda_native::media::Callbacks;
use serde_json::Value;
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

pub(super) struct AuthorCallbacks<'a> {
    pub(super) parent: &'a mut dyn Callbacks,
    pub(super) disc: u64,
    pub(super) total: u64,
    pub(super) oversized: u64,
    pub(super) coordinate_warnings: u64,
    pub(super) phase: &'static str,
    pub(super) menu_stop: Option<&'a AtomicBool>,
    pub(super) menu_events: Option<&'a MenuChannel>,
}

impl Callbacks for AuthorCallbacks<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        self.drain_menus();
        if let Some(number) = oversized_still_picture_number(text) {
            self.oversized += 1;
            self.parent
                .emit(1, &format!("[menu-cover] oversized {number}"));
        } else {
            self.parent.emit(stream, text);
            if text
                .trim()
                .starts_with("WARN: Button y coordinates are odd for button ")
            {
                self.coordinate_warnings += 1;
                if self.coordinate_warnings == 1 {
                    crate::task_log::warning(
                        self.parent,
                        "button_coordinates_seen",
                        1,
                        &self.disc.to_string(),
                    );
                }
            }
            if let Some((phase, name)) = author_phase(text) {
                // Raw author evidence remains unchanged in the detailed log.
                // Stage events are additional UI information, never fake progress.
                if phase != self.phase || !name.is_empty() {
                    self.phase = phase;
                    crate::task_log::emit(self.parent, phase, self.disc, self.total, name, text);
                }
            }
        }
    }

    fn cancelled(&mut self) -> bool {
        self.drain_menus();
        let cancelled = self.parent.cancelled();
        if cancelled && let Some(stop) = self.menu_stop {
            stop.store(true, Ordering::Release);
        }
        cancelled
    }

    fn progress(&mut self, completed: u64, total: u64) {
        self.drain_menus();
        if total == 0 {
            return;
        }
        let start = 65_000 + self.disc.saturating_sub(1) * 30_000 / self.total.max(1);
        let span = 30_000 / self.total.max(1);
        self.parent
            .progress(start + span * completed.min(total) / total, 100_000);
    }
}

impl AuthorCallbacks<'_> {
    fn drain_menus(&mut self) {
        if let Some(channel) = self.menu_events {
            channel.drain(self.parent);
        }
    }

    pub(super) fn finish(&mut self) {
        self.drain_menus();
        let disc = format!("{}", self.disc);
        if self.oversized > 0 {
            crate::task_log::emit(
                self.parent,
                "oversized_summary",
                self.oversized,
                self.total,
                &disc,
                "",
            );
        }
        if self.coordinate_warnings > 0 {
            crate::task_log::warning(
                self.parent,
                "button_coordinates",
                self.coordinate_warnings,
                &disc,
            );
        }
    }
}

pub(super) struct MenuAssets {
    pub(super) result: Result<Vec<String>, String>,
    pub(super) diagnostics: Vec<Value>,
}

pub(super) enum MenuEvent {
    Log(i32, String),
    Progress(u64, u64),
    Finished(MenuAssets),
}

pub(super) struct MenuChannel {
    receiver: mpsc::Receiver<MenuEvent>,
    pub(super) assets: RefCell<Option<MenuAssets>>,
    disc: usize,
    total: usize,
}

impl MenuChannel {
    pub(super) fn new(receiver: mpsc::Receiver<MenuEvent>, disc: usize, total: usize) -> Self {
        Self {
            receiver,
            assets: RefCell::new(None),
            disc,
            total,
        }
    }

    fn forward(&self, event: MenuEvent, caller: &mut dyn Callbacks) {
        match event {
            MenuEvent::Log(stream, text) => caller.emit(stream, &text),
            MenuEvent::Progress(completed, total) => {
                let disc = self.disc as u64;
                let discs = self.total.max(1) as u64;
                let start = 65_000 + disc.saturating_sub(1) * 30_000 / discs;
                let span = 30_000 / discs;
                let Some(portion) = (span * completed.min(total)).checked_div(total) else {
                    return;
                };
                caller.progress(start + portion, 100_000);
            }
            MenuEvent::Finished(assets) => *self.assets.borrow_mut() = Some(assets),
        }
    }

    pub(super) fn drain(&self, caller: &mut dyn Callbacks) {
        while self.assets.borrow().is_none() {
            match self.receiver.try_recv() {
                Ok(event) => self.forward(event, caller),
                Err(_) => break,
            }
        }
    }
}

pub(super) struct MenuWorkerCallbacks<'a> {
    pub(super) stop: &'a AtomicBool,
    pub(super) send: &'a mpsc::SyncSender<MenuEvent>,
}

impl Callbacks for MenuWorkerCallbacks<'_> {
    fn emit(&mut self, stream: i32, text: &str) {
        let _ = self.send.send(MenuEvent::Log(stream, text.into()));
    }

    fn cancelled(&mut self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    fn progress(&mut self, completed: u64, total: u64) {
        let _ = self.send.send(MenuEvent::Progress(completed, total));
    }
}

pub(super) struct StopMenus<'a>(pub(super) &'a AtomicBool);
impl Drop for StopMenus<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

pub(super) fn receive_menu(
    channel: &MenuChannel,
    caller: &mut dyn Callbacks,
    stop: &AtomicBool,
) -> Result<MenuAssets, String> {
    loop {
        if let Some(assets) = channel.assets.borrow_mut().take() {
            return Ok(assets);
        }
        if caller.cancelled() {
            stop.store(true, Ordering::Release);
            return Err("Build cancelled".into());
        }
        match channel.receiver.recv_timeout(Duration::from_millis(20)) {
            Ok(event) => channel.forward(event, caller),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                stop.store(true, Ordering::Release);
                return Err("Menu asset worker stopped unexpectedly".into());
            }
        }
    }
}

fn author_phase(line: &str) -> Option<(&'static str, &str)> {
    let line = line.trim().strip_prefix("[INF]")?.trim();
    if let Some(path) = line.strip_prefix("Auditing MLP file ") {
        Some((
            "author_audio",
            path.rsplit(['/', '\\']).next().unwrap_or(path),
        ))
    } else if line.starts_with("Searching MLP layout for file ") {
        Some(("author_layout", ""))
    } else if line.starts_with("Writing ISO image")
        || line.starts_with("Creating ISO with the in-process C ISO writer")
    {
        Some(("author_iso", ""))
    } else if line.starts_with("Creating ASVS") {
        Some(("author_stills", ""))
    } else if line.starts_with("Creating ") {
        Some(("author_navigation", ""))
    } else {
        None
    }
}

fn oversized_still_picture_number(line: &str) -> Option<u32> {
    let trimmed = line.trim();
    let line = trimmed.strip_prefix("[ERR]").unwrap_or(trimmed).trim();
    let number = line.strip_prefix("Exceeding stillpic buffer limit (2 MB) at pict #")?;
    number.strip_suffix('.').unwrap_or(number).parse().ok()
}
