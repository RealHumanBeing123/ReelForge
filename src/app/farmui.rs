//! Render farm over a shared folder, and the RAW converter settings. Child module of `app`.
//! Any machine running ReelForge with "Work for the render farm" on picks up waiting parts;
//! the machine that submitted the job joins the finished parts.
use super::*;
use crate::farm as fm;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone)]
struct FarmJob {
    name: String,
    owner: String,
    parts: usize,
    /// Final file, a path on the submitting machine.
    output: String,
    preset: render::ExportPreset,
}

type Worker = (Receiver<RenderMsg>, Arc<AtomicBool>, PathBuf, usize, Instant);

pub struct Farm {
    pub open: bool,
    pub raw_open: bool,
    parts: usize,
    msg: String,
    worker: Option<Worker>,
    scanned: Instant,
    jobs: Vec<(PathBuf, FarmJob, Vec<fm::PartState>, bool)>,
    asm_rx: Option<Receiver<(PathBuf, Result<(), String>)>>,
    assembling: Option<PathBuf>,
}

impl Farm {
    pub fn new() -> Farm {
        Farm { open: false, raw_open: false, parts: 4, msg: String::new(), worker: None, scanned: Instant::now() - Duration::from_secs(60), jobs: vec![], asm_rx: None, assembling: None }
    }
}

fn read_job(dir: &Path) -> Option<FarmJob> {
    serde_json::from_str(&std::fs::read_to_string(dir.join("job.json")).ok()?).ok()
}

impl ReelForge {
    fn farm_jobs_dir(&self) -> Option<PathBuf> {
        let d = self.ext.cfg.farm_dir.trim();
        if d.is_empty() {
            None
        } else {
            Some(PathBuf::from(d).join("jobs"))
        }
    }

    /// Split the timeline into parts and put them in the farm folder.
    fn farm_submit(&mut self) {
        let Some(jobs) = self.farm_jobs_dir() else {
            self.farm.msg = "Choose the shared farm folder first.".into();
            return;
        };
        let total = self.project.duration();
        if total < 0.2 || self.project.clips.is_empty() {
            self.farm.msg = "Nothing to render.".into();
            return;
        }
        let mut st = self.settings.clone();
        st.normalize();
        let Some(out) = rfd::FileDialog::new().set_file_name(format!("farm_render.{}", st.container.ext())).save_file() else { return };
        let fps = st.fps.max(1);
        let parts = fm::split(total, fps, self.farm.parts);
        let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let dir = jobs.join(format!("job_{id}"));
        if let Err(e) = std::fs::create_dir_all(dir.join("out")) {
            self.farm.msg = format!("Could not create the job folder: {e}");
            return;
        }
        for (i, (a, b)) in parts.iter().enumerate() {
            let p = self.project.range(*a, *b);
            if let Err(e) = serde_json::to_string(&p).map_err(|e| e.to_string()).and_then(|s| std::fs::write(fm::path_of(&dir, i + 1, "rforge"), s).map_err(|e| e.to_string())) {
                self.farm.msg = format!("Could not write part {}: {e}", i + 1);
                return;
            }
        }
        let job = FarmJob { name: format!("job_{id}"), owner: super::collab::who(), parts: parts.len(), output: out.to_string_lossy().into_owned(), preset: render::ExportPreset::from_settings("farm", &st) };
        // job.json goes last: workers ignore a job folder until it exists
        match serde_json::to_string_pretty(&job).map_err(|e| e.to_string()).and_then(|s| std::fs::write(dir.join("job.json"), s).map_err(|e| e.to_string())) {
            Ok(()) => self.farm.msg = format!("Submitted {} in {} parts. Machines working for the farm will pick them up.", job.name, job.parts),
            Err(e) => self.farm.msg = format!("Could not write job.json: {e}"),
        }
        self.farm.scanned = Instant::now() - Duration::from_secs(60);
    }

    pub(super) fn farm_poll(&mut self, ctx: &egui::Context) {
        // a finished assembly
        if let Some(rx) = &self.farm.asm_rx {
            if let Ok((dir, r)) = rx.try_recv() {
                self.farm.asm_rx = None;
                self.farm.assembling = None;
                match r {
                    Ok(()) => {
                        let _ = std::fs::write(dir.join("assembled"), "ok");
                        self.farm.msg = format!("Joined {}.", dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                    }
                    Err(e) => {
                        let _ = std::fs::write(dir.join("assemble_failed"), &e);
                        self.farm.msg = format!("Joining failed: {e}");
                    }
                }
            }
        }
        // our own part render
        if let Some((rx, _, dir, k, started)) = &self.farm.worker {
            let mut fin: Option<Result<(), String>> = None;
            while let Ok(m) = rx.try_recv() {
                match m {
                    RenderMsg::Done => fin = Some(Ok(())),
                    RenderMsg::Failed(e) => fin = Some(Err(e.lines().last().unwrap_or("failed").to_string())),
                    RenderMsg::Cancelled => fin = Some(Err("cancelled".into())),
                    RenderMsg::Progress(_) => {}
                }
            }
            let (dir, k, secs) = (dir.clone(), *k, started.elapsed().as_secs_f32());
            match fin {
                Some(Ok(())) => {
                    let _ = std::fs::write(fm::path_of(&dir, k, "done"), format!("{} {secs:.0}s", super::collab::who()));
                    self.farm.worker = None;
                    self.farm.scanned = Instant::now() - Duration::from_secs(60);
                }
                Some(Err(e)) => {
                    let _ = std::fs::write(fm::path_of(&dir, k, "failed"), e);
                    self.farm.worker = None;
                }
                None => ctx.request_repaint_after(Duration::from_millis(500)),
            }
        }
        let watching = self.ext.cfg.farm_worker || self.farm.open;
        let Some(jobs_dir) = self.farm_jobs_dir() else { return };
        if !watching || self.farm.scanned.elapsed() < Duration::from_secs(4) {
            if watching {
                ctx.request_repaint_after(Duration::from_secs(2));
            }
            return;
        }
        self.farm.scanned = Instant::now();
        ctx.request_repaint_after(Duration::from_secs(4));

        // read the state of every job
        let me = super::collab::who();
        let mut list = vec![];
        if let Ok(rd) = std::fs::read_dir(&jobs_dir) {
            for e in rd.flatten() {
                let dir = e.path();
                if let Some(job) = read_job(&dir) {
                    let states: Vec<fm::PartState> = (1..=job.parts).map(|k| fm::part_state(&dir, k)).collect();
                    let assembled = dir.join("assembled").exists();
                    list.push((dir, job, states, assembled));
                }
            }
        }
        list.sort_by(|a, b| a.0.cmp(&b.0));

        // join finished jobs that we submitted
        if self.farm.assembling.is_none() {
            for (dir, job, states, assembled) in &list {
                if job.owner == me && !*assembled && !dir.join("assemble_failed").exists() && states.iter().all(|s| *s == fm::PartState::Done) {
                    let parts: Vec<PathBuf> = (1..=job.parts).map(|k| dir.join("out").join(format!("{}.{}", fm::part_name(k), job.preset.container.ext()))).collect();
                    let (out, dir2) = (PathBuf::from(&job.output), dir.clone());
                    let (tx, rx) = channel();
                    self.farm.asm_rx = Some(rx);
                    self.farm.assembling = Some(dir.clone());
                    self.farm.msg = format!("Joining {} parts...", job.parts);
                    let ctx2 = ctx.clone();
                    std::thread::spawn(move || {
                        let r = fm::concat(&parts, &out, &dir2);
                        let _ = tx.send((dir2, r));
                        ctx2.request_repaint();
                    });
                    break;
                }
            }
        }

        // pick up a waiting part (only when this machine is idle and the farm switch is on)
        if self.ext.cfg.farm_worker && self.farm.worker.is_none() && self.collab.job.is_none() && self.render_started.is_none() {
            'find: for (dir, job, states, _) in &list {
                for (i, s) in states.iter().enumerate() {
                    let k = i + 1;
                    if *s != fm::PartState::Waiting || !fm::path_of(dir, k, "rforge").exists() {
                        continue;
                    }
                    if !fm::try_claim(dir, k, &me) {
                        continue;
                    }
                    let proj = std::fs::read_to_string(fm::path_of(dir, k, "rforge")).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Project>(&s).map_err(|e| e.to_string()));
                    match proj {
                        Ok(p) => {
                            let mut st = RenderSettings::default();
                            job.preset.apply(&mut st);
                            st.output = Some(dir.join("out").join(format!("{}.{}", fm::part_name(k), st.container.ext())));
                            st.use_range = false;
                            st.burn_subs = true;
                            let (rx, cancel) = render::start(ctx.clone(), p, st);
                            self.farm.worker = Some((rx, cancel, dir.clone(), k, Instant::now()));
                            self.farm.msg = format!("Rendering part {k} of {}", job.name);
                        }
                        Err(e) => {
                            let _ = std::fs::write(fm::path_of(dir, k, "failed"), format!("could not read part: {e}"));
                        }
                    }
                    break 'find;
                }
            }
        }
        self.farm.jobs = list;
    }

    pub(super) fn win_farm(&mut self, ctx: &egui::Context) {
        if !self.farm.open {
            return;
        }
        let mut open = true;
        let mut changed = false;
        let (mut submit, mut retry): (bool, Option<usize>) = (false, None);
        egui::Window::new("Render farm").open(&mut open).default_width(460.0).show(ctx, |ui| {
            ui.label(egui::RichText::new("Several computers share one folder (a network share or synced folder). The timeline is split into parts, each machine renders parts, and the submitting machine joins them. Every machine must be able to open your media at the same path, so keep media on the shared drive.").weak().small());
            ui.horizontal(|ui| {
                ui.label("Farm folder");
                ui.monospace(if self.ext.cfg.farm_dir.is_empty() { "(none)".to_string() } else { elide(&self.ext.cfg.farm_dir, 44) });
                if ui.button("Choose...").clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        self.ext.cfg.farm_dir = d.to_string_lossy().into_owned();
                        changed = true;
                    }
                }
            });
            changed |= ui.checkbox(&mut self.ext.cfg.farm_worker, "Work for the render farm (this computer renders waiting parts while ReelForge is open and idle)").changed();
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Split into");
                ui.add(egui::DragValue::new(&mut self.farm.parts).range(2..=64).suffix(" parts"));
                if ui.add_enabled(!self.ext.cfg.farm_dir.is_empty(), egui::Button::new("Submit current export settings...")).clicked() {
                    submit = true;
                }
            });
            ui.label(egui::RichText::new("Uses the settings in the Export window. A part boundary cuts clips there, so fades and transitions that straddle a boundary are cut; put boundaries away from them by choosing a different number of parts.").weak().small());
            ui.separator();
            if self.farm.jobs.is_empty() {
                ui.label(egui::RichText::new("No jobs in the farm folder.").weak());
            }
            for (i, (_, job, states, assembled)) in self.farm.jobs.iter().enumerate() {
                let n = |s: fm::PartState| states.iter().filter(|x| **x == s).count();
                let failed = n(fm::PartState::Failed);
                ui.horizontal(|ui| {
                    ui.monospace(&job.name);
                    ui.label(format!("{} done, {} rendering, {} waiting", n(fm::PartState::Done), n(fm::PartState::Rendering), n(fm::PartState::Waiting)));
                    if failed > 0 {
                        ui.colored_label(Color32::from_rgb(255, 120, 100), format!("{failed} failed"));
                        if ui.small_button("Retry failed").clicked() {
                            retry = Some(i);
                        }
                    }
                    if *assembled {
                        ui.label("joined");
                    }
                });
            }
            if !self.farm.msg.is_empty() {
                ui.separator();
                ui.label(&self.farm.msg);
            }
        });
        if changed {
            self.ext.save_cfg();
        }
        if submit {
            self.farm_submit();
        }
        if let Some(i) = retry {
            if let Some((dir, job, _, _)) = self.farm.jobs.get(i) {
                fm::retry_failed(dir, job.parts);
                self.farm.scanned = Instant::now() - Duration::from_secs(60);
            }
        }
        self.farm.open = open;
    }

    pub(super) fn win_raw(&mut self, ctx: &egui::Context) {
        if !self.farm.raw_open {
            return;
        }
        let mut open = true;
        let mut changed = false;
        egui::Window::new("RAW converter").open(&mut open).default_width(480.0).show(ctx, |ui| {
            ui.label(egui::RichText::new("ReelForge cannot decode camera RAW itself (.braw, .r3d, .ari, .crm and similar). Give it the command-line tool from your camera maker and it will convert each RAW file to an editable file when you import it. Use {in} and {out} where the files go.").weak().small());
            ui.label("Command");
            changed |= ui.add(egui::TextEdit::singleline(&mut self.ext.cfg.raw_cmd).desired_width(f32::INFINITY).hint_text("tool.exe --input \"{in}\" --output \"{out}\" ...")).changed();
            ui.horizontal(|ui| {
                ui.label("Converted file type");
                changed |= ui.add(egui::TextEdit::singleline(&mut self.ext.cfg.raw_ext).desired_width(60.0).hint_text("mov")).changed();
            });
            ui.label(egui::RichText::new("Example shape only (check your tool's own documentation for the real options): REDline.exe --i \"{in}\" --o \"{out}\" --format 201. The tool must be installed on your computer; ReelForge does not include any. The converted files are kept in %APPDATA%\\ReelForge\\raw.").weak().small());
        });
        if changed {
            self.ext.save_cfg();
        }
        self.farm.raw_open = open;
    }
}
