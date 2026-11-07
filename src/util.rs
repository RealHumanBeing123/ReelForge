use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Locate an FFmpeg tool: prefer `<name>.exe` next to our executable, else PATH.
pub fn tool(name: &str) -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(format!("{name}.exe"));
            if p.exists() {
                return p;
            }
        }
    }
    PathBuf::from(name)
}

/// Build a command that never flashes a console window on Windows.
pub fn command(name: &str) -> Command {
    let mut c = Command::new(tool(name));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

pub fn ffmpeg_available() -> bool {
    command("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn fmt_time(t: f64) -> String {
    let t = t.max(0.0);
    let m = (t / 60.0) as u32;
    let s = t - m as f64 * 60.0;
    format!("{:02}:{:05.2}", m, s)
}
