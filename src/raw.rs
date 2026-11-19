//! Camera RAW and other formats FFmpeg cannot decode: convert them with the camera maker's own
//! command-line tool (REDline, Blackmagic RAW tools, ARRI Reference Tool...) that the user
//! configures, then edit the converted file. ReelForge itself decodes none of these formats.
use crate::util::command;
use std::path::Path;
use std::process::Stdio;

pub const RAW_EXTS: &[&str] = &["braw", "r3d", "ari", "arx", "crm", "nev", "rmf", "cine"];

pub fn is_raw(p: &Path) -> bool {
    p.extension().map_or(false, |x| RAW_EXTS.contains(&x.to_string_lossy().to_lowercase().as_str()))
}

/// Split a command template into arguments, honouring double quotes.
pub fn split_args(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut q, mut any) = (vec![], String::new(), false, false);
    for ch in s.chars() {
        match ch {
            '"' => {
                q = !q;
                any = true;
            }
            c if c.is_whitespace() && !q => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Fill `{in}` and `{out}` in the template. The first word is the program.
pub fn build(template: &str, input: &Path, output: &Path) -> Option<(String, Vec<String>)> {
    let mut v = split_args(template);
    if v.is_empty() || !template.contains("{in}") || !template.contains("{out}") {
        return None;
    }
    let (i, o) = (input.to_string_lossy().into_owned(), output.to_string_lossy().into_owned());
    for a in v.iter_mut() {
        *a = a.replace("{in}", &i).replace("{out}", &o);
    }
    let prog = v.remove(0);
    Some((prog, v))
}

pub fn convert(template: &str, input: &Path, output: &Path) -> Result<(), String> {
    let (prog, args) = build(template, input, output).ok_or("Set a converter command containing {in} and {out} first (Tools > RAW converter).")?;
    if let Some(d) = output.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let o = if prog.eq_ignore_ascii_case("ffmpeg") || prog.eq_ignore_ascii_case("ffmpeg.exe") { command("ffmpeg") } else { std::process::Command::new(&prog) }
        .args(&args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Could not run {prog}: {e}"))?;
    if !o.status.success() {
        let msg = String::from_utf8_lossy(&o.stderr);
        return Err(format!("{prog} failed: {}", msg.lines().last().unwrap_or("no message")));
    }
    if std::fs::metadata(output).map(|m| m.len()).unwrap_or(0) == 0 {
        return Err(format!("{prog} finished but wrote no file."));
    }
    Ok(())
}
