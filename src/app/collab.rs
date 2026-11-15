//! Watch folders, project locking / conflict protection and version history.
//! Child module of `app` (uses its private fields).
use super::*;
use crate::interchange;
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::time::{Duration, SystemTime};

type Job = (Receiver<RenderMsg>, Arc<AtomicBool>, String, Instant);

pub struct Collab {
    pub watch_open: bool,
    pub versions_open: bool,
    pub version_note: String,
    versions: Vec<(PathBuf, String)>,
    versions_for: Option<PathBuf>,
    session: String,
    /// Modification time of the project file when we last read or wrote it.
    proj_mtime: Option<SystemTime>,
    last_beat: Instant,
    watch_last: Instant,
    seen: HashMap<PathBuf, u64>,
    imported: HashSet<PathBuf>,
    rendered: HashMap<PathBuf, SystemTime>,
    queue: VecDeque<(String, Project, RenderSettings)>,
    job: Option<Job>,
    pub log: VecDeque<String>,
}

impl Collab {
    pub fn new() -> Collab {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        Collab {
            watch_open: false,
            versions_open: false,
            version_note: String::new(),
            versions: vec![],
            versions_for: None,
            session: format!("{}-{nanos:x}", std::process::id()),
            proj_mtime: None,
            last_beat: Instant::now(),
            watch_last: Instant::now(),
            seen: HashMap::new(),
            imported: HashSet::new(),
            rendered: HashMap::new(),
            queue: VecDeque::new(),
            job: None,
            log: VecDeque::new(),
        }
    }
    fn say(&mut self, m: String) {
        self.log.push_front(m);
        self.log.truncate(30);
    }
}

fn who() -> String {
    let user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_else(|_| "someone".into());
    let host = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "this computer".into());
    format!("{user}@{host}")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn lock_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".lock");
    PathBuf::from(s)
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn safe_name(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' }).collect();
    t.trim_matches('_').chars().take(40).collect()
}

/// A lock younger than this, written by another session, means the project is probably open elsewhere.
const LOCK_FRESH: Duration = Duration::from_secs(5 * 60);
const HEARTBEAT: Duration = Duration::from_secs(60);

impl ReelForge {
    // ------------------------------------------------------------ locking and conflicts

    fn write_lock(&mut self, p: &Path) {
        let _ = std::fs::write(lock_path(p), format!("{}\n{}\n{}\n", self.collab.session, who(), now_secs()));
    }

    /// Called after a project file was opened: warn if someone else seems to have it open.
    pub(super) fn collab_opened(&mut self, p: &Path) {
        self.collab.proj_mtime = mtime(p);
        if let Ok(text) = std::fs::read_to_string(lock_path(p)) {
            let mut l = text.lines();
            let (sess, name) = (l.next().unwrap_or(""), l.next().unwrap_or("someone"));
            let fresh = mtime(&lock_path(p)).and_then(|t| t.elapsed().ok()).map_or(false, |e| e < LOCK_FRESH);
            if sess != self.collab.session && fresh {
                self.status = format!("Warning: this project looks open by {name} right now. Saving will not overwrite their changes; it makes a conflict copy instead.");
            }
        }
        self.write_lock(p);
    }

    /// Where a save should go: the project file, or a conflict copy if the file changed on disk
    /// since we opened or last saved it (another person or session wrote to it).
    pub(super) fn collab_save_target(&mut self, p: &Path) -> (PathBuf, bool) {
        let changed = match (self.collab.proj_mtime, mtime(p)) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        };
        if !changed {
            return (p.to_path_buf(), false);
        }
        let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into());
        let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
        let mut n = 1;
        loop {
            let name = if n == 1 { format!("{stem} (conflict {}).rforge", safe_name(&who())) } else { format!("{stem} (conflict {} {n}).rforge", safe_name(&who())) };
            let c = dir.join(name);
            if !c.exists() {
                return (c, true);
            }
            n += 1;
        }
    }

    pub(super) fn collab_saved(&mut self, p: &Path, was_conflict: bool) {
        if was_conflict {
            return; // the original stays as it was on disk; our view of it is unchanged
        }
        self.collab.proj_mtime = mtime(p);
        self.write_lock(p);
    }

    // ------------------------------------------------------------ version history

    fn versions_dir(&self) -> Option<PathBuf> {
        let p = self.project_path.as_ref()?;
        let stem = p.file_stem()?.to_string_lossy().into_owned();
        Some(p.parent()?.join(".reelforge_versions").join(stem))
    }

    fn refresh_versions(&mut self) {
        self.collab.versions.clear();
        self.collab.versions_for = self.project_path.clone();
        let Some(dir) = self.versions_dir() else { return };
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut v: Vec<(PathBuf, String)> = rd
            .flatten()
            .filter(|e| e.path().extension().map_or(false, |x| x == "rforge"))
            .map(|e| {
                let name = e.path().file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let (ts, note) = name.split_once('_').unwrap_or((&name, ""));
                let when = ts.parse::<u64>().map(interchange::iso_utc).unwrap_or_else(|_| ts.to_string());
                (e.path(), if note.is_empty() { when } else { format!("{when}  {}", note.replace('_', " ")) })
            })
            .collect();
        v.sort_by(|a, b| b.0.cmp(&a.0));
        self.collab.versions = v;
    }

    pub(super) fn save_version(&mut self) {
        let Some(dir) = self.versions_dir() else {
            self.status = "Save the project first, then you can keep versions of it.".into();
            return;
        };
        let note = safe_name(&self.collab.version_note);
        let name = if note.is_empty() { format!("{}.rforge", now_secs()) } else { format!("{}_{note}.rforge", now_secs()) };
        let r = std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|_| serde_json::to_string_pretty(&self.project).map_err(|e| e.to_string())).and_then(|s| std::fs::write(dir.join(&name), s).map_err(|e| e.to_string()));
        self.status = match r {
            Ok(()) => {
                self.collab.version_note.clear();
                self.refresh_versions();
                // keep the newest 60
                for (p, _) in self.collab.versions.clone().into_iter().skip(60) {
                    let _ = std::fs::remove_file(p);
                }
                "Version saved.".into()
            }
            Err(e) => format!("Could not save version: {e}"),
        };
    }

    fn restore_version(&mut self, path: &Path) {
        match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Project>(&s).map_err(|e| e.to_string())) {
            Ok(p) => {
                self.push_undo();
                self.project = p;
                self.selected = None;
                self.revision += 1;
                self.preview_dirty = true;
                self.status = "Version restored (Edit > Undo goes back).".into();
            }
            Err(e) => self.status = format!("Could not read that version: {e}"),
        }
    }

    pub(super) fn win_versions(&mut self, ctx: &egui::Context) {
        if !self.collab.versions_open {
            return;
        }
        if self.collab.versions_for != self.project_path {
            self.refresh_versions();
        }
        let mut open = true;
        let (mut save, mut restore, mut delete): (bool, Option<PathBuf>, Option<PathBuf>) = (false, None, None);
        egui::Window::new("Version history").open(&mut open).default_width(420.0).show(ctx, |ui| {
            if self.project_path.is_none() {
                ui.label("Save the project first.");
                return;
            }
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.collab.version_note);
                if ui.button("Save version").clicked() {
                    save = true;
                }
            });
            ui.label(egui::RichText::new("Type a short note (optional). Versions are kept next to the project; the newest 60 are kept.").weak().small());
            ui.separator();
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                if self.collab.versions.is_empty() {
                    ui.label(egui::RichText::new("No saved versions yet.").weak());
                }
                for (p, label) in &self.collab.versions {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        if ui.small_button("Restore").clicked() {
                            restore = Some(p.clone());
                        }
                        if ui.small_button("Delete").clicked() {
                            delete = Some(p.clone());
                        }
                    });
                }
            });
        });
        if save {
            self.save_version();
        }
        if let Some(p) = restore {
            self.restore_version(&p);
        }
        if let Some(p) = delete {
            let _ = std::fs::remove_file(p);
            self.refresh_versions();
        }
        self.collab.versions_open = open;
    }

    // ------------------------------------------------------------ watch folders

    fn preset_by_name(&self, name: &str) -> Option<render::ExportPreset> {
        let mut all = render::ExportPreset::builtin();
        all.extend(self.ext.cfg.export_presets.clone());
        all.iter().find(|p| p.name == name).cloned().or_else(|| all.into_iter().next())
    }

    fn stable(&mut self, p: &Path) -> bool {
        let len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let prev = self.collab.seen.insert(p.to_path_buf(), len);
        len > 0 && prev == Some(len)
    }

    pub(super) fn collab_poll(&mut self, ctx: &egui::Context) {
        // heartbeat for the project lock
        if self.collab.last_beat.elapsed() >= HEARTBEAT {
            self.collab.last_beat = Instant::now();
            if let Some(p) = self.project_path.clone() {
                if p.exists() {
                    self.write_lock(&p);
                }
            }
        }

        // current auto-render job
        if let Some((rx, _, name, started)) = &self.collab.job {
            let mut finished: Option<String> = None;
            while let Ok(m) = rx.try_recv() {
                match m {
                    RenderMsg::Done => finished = Some(format!("Rendered {name} in {:.0}s", started.elapsed().as_secs_f32())),
                    RenderMsg::Failed(e) => finished = Some(format!("Render of {name} failed: {}", e.lines().last().unwrap_or(""))),
                    RenderMsg::Cancelled => finished = Some(format!("Render of {name} cancelled")),
                    RenderMsg::Progress(_) => {}
                }
            }
            if let Some(m) = finished {
                self.collab.job = None;
                self.collab.say(m);
            } else {
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
        if self.collab.job.is_none() {
            if let Some((name, proj, mut st)) = self.collab.queue.pop_front() {
                st.normalize();
                let (rx, cancel) = render::start(ctx.clone(), proj, st);
                self.collab.job = Some((rx, cancel, name.clone(), Instant::now()));
                self.collab.say(format!("Rendering {name}..."));
            }
        }

        if !self.ext.cfg.watch_on || self.collab.watch_last.elapsed() < Duration::from_secs(3) {
            if self.ext.cfg.watch_on {
                ctx.request_repaint_after(Duration::from_secs(1));
            }
            return;
        }
        self.collab.watch_last = Instant::now();
        ctx.request_repaint_after(Duration::from_secs(3));

        // media folder: import new, finished files
        let mf = self.ext.cfg.watch_media.clone();
        if !mf.is_empty() {
            if let Ok(rd) = std::fs::read_dir(&mf) {
                for e in rd.flatten() {
                    let p = e.path();
                    let ok = p.is_file() && p.extension().map_or(false, |x| MEDIA_EXTS.contains(&x.to_string_lossy().to_lowercase().as_str()));
                    if !ok || self.collab.imported.contains(&p) || self.project.media.iter().any(|m| m.path == p) {
                        continue;
                    }
                    if self.stable(&p) {
                        self.collab.imported.insert(p.clone());
                        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        self.import_path(ctx, p);
                        self.collab.say(format!("Imported {name}"));
                    }
                }
            }
        }

        // render folder: render finished .rforge projects with the chosen preset
        let rf = self.ext.cfg.watch_render.clone();
        if !rf.is_empty() {
            if let Ok(rd) = std::fs::read_dir(&rf) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().map_or(true, |x| x != "rforge") {
                        continue;
                    }
                    let Some(mt) = mtime(&p) else { continue };
                    if self.collab.rendered.get(&p) == Some(&mt) || !self.stable(&p) {
                        continue;
                    }
                    self.collab.rendered.insert(p.clone(), mt);
                    let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Project>(&s).map_err(|e| e.to_string())) {
                        Ok(proj) => {
                            let mut st = self.settings.clone();
                            if let Some(pr) = self.preset_by_name(&self.ext.cfg.watch_preset) {
                                pr.apply(&mut st);
                            }
                            let out_dir = PathBuf::from(&rf).join("rendered");
                            let _ = std::fs::create_dir_all(&out_dir);
                            st.output = Some(out_dir.join(format!("{name}.{}", st.container.ext())));
                            st.use_range = false;
                            self.collab.queue.push_back((name.clone(), proj, st));
                            self.collab.say(format!("Queued {name}"));
                        }
                        Err(err) => self.collab.say(format!("Could not read {name}: {err}")),
                    }
                }
            }
        }
    }

    pub(super) fn win_watch(&mut self, ctx: &egui::Context) {
        if !self.collab.watch_open {
            return;
        }
        let mut open = true;
        let mut changed = false;
        let presets: Vec<String> = {
            let mut all = render::ExportPreset::builtin();
            all.extend(self.ext.cfg.export_presets.clone());
            all.into_iter().map(|p| p.name).collect()
        };
        egui::Window::new("Watch folders").open(&mut open).default_width(440.0).show(ctx, |ui| {
            changed |= ui.checkbox(&mut self.ext.cfg.watch_on, "Watching (only while ReelForge is open)").changed();
            ui.separator();
            ui.label("Import folder: new media files are imported once they finish copying.");
            ui.horizontal(|ui| {
                ui.monospace(if self.ext.cfg.watch_media.is_empty() { "(none)".to_string() } else { elide(&self.ext.cfg.watch_media, 48) });
                if ui.button("Choose...").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.ext.cfg.watch_media = d.to_string_lossy().into_owned();
                        changed = true;
                    }
                }
                if ui.button("Clear").clicked() {
                    self.ext.cfg.watch_media.clear();
                    changed = true;
                }
            });
            ui.add_space(6.0);
            ui.label("Render folder: .rforge projects dropped here are exported to a 'rendered' subfolder.");
            ui.horizontal(|ui| {
                ui.monospace(if self.ext.cfg.watch_render.is_empty() { "(none)".to_string() } else { elide(&self.ext.cfg.watch_render, 48) });
                if ui.button("Choose...").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.ext.cfg.watch_render = d.to_string_lossy().into_owned();
                        changed = true;
                    }
                }
                if ui.button("Clear").clicked() {
                    self.ext.cfg.watch_render.clear();
                    changed = true;
                }
            });
            egui::ComboBox::from_label("Export preset").selected_text(if self.ext.cfg.watch_preset.is_empty() { "(current export settings)".to_string() } else { self.ext.cfg.watch_preset.clone() }).show_ui(ui, |ui| {
                if ui.selectable_label(self.ext.cfg.watch_preset.is_empty(), "(current export settings)").clicked() {
                    self.ext.cfg.watch_preset.clear();
                    changed = true;
                }
                for p in &presets {
                    if ui.selectable_label(&self.ext.cfg.watch_preset == p, p).clicked() {
                        self.ext.cfg.watch_preset = p.clone();
                        changed = true;
                    }
                }
            });
            ui.separator();
            if self.collab.job.is_some() || !self.collab.queue.is_empty() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Rendering, {} waiting", self.collab.queue.len()));
                    if ui.small_button("Stop").clicked() {
                        if let Some((_, c, _, _)) = &self.collab.job {
                            c.store(true, Ordering::Relaxed);
                        }
                        self.collab.queue.clear();
                    }
                });
            }
            egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                for l in &self.collab.log {
                    ui.label(egui::RichText::new(l).small());
                }
            });
        });
        if changed {
            self.ext.save_cfg();
        }
        self.collab.watch_open = open;
    }
}
