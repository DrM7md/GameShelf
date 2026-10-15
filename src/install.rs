//! Installing emulators from GameShelf: find the latest official Windows build,
//! download it, unpack it into `GameShelf\Emulators\<Emulator>`, and find its program.
//!
//! Downloads come only from each project's own release page (GitHub releases,
//! ppsspp.org, buildbot.libretro.com). Dolphin's site doesn't allow automated
//! downloads, so for Dolphin the download page is opened instead.

use crate::workspace;
use eframe::egui;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

/// Bigger than any emulator build; stops a runaway download.
const MAX_DOWNLOAD: u64 = 2 * 1024 * 1024 * 1024;

pub enum Msg {
    Status(String),
    Progress {
        done: u64,
        total: Option<u64>,
    },
    /// The emulator's program, ready to use.
    Done(Result<PathBuf, String>),
}

/// Can GameShelf install this emulator by itself?
pub fn can_install(emulator: &str) -> bool {
    matches!(
        emulator,
        "RetroArch" | "DuckStation" | "PCSX2" | "PPSSPP" | "RPCS3" | "Flycast"
    )
}

struct Download {
    url: String,
    file_name: String,
    version: String,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .user_agent("GameShelf")
        .build()
}

fn get_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let resp = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
    let mut text = String::new();
    resp.into_reader()
        .take(8 * 1024 * 1024)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    Ok(text)
}

/// The newest asset of a GitHub release whose name passes `pick`.
fn github(
    agent: &ureq::Agent,
    repo: &str,
    tag: Option<&str>,
    pick: fn(&str) -> bool,
) -> Result<Download, String> {
    let url = match tag {
        Some(tag) => format!("https://api.github.com/repos/{repo}/releases/tags/{tag}"),
        None => format!("https://api.github.com/repos/{repo}/releases/latest"),
    };
    let json: serde_json::Value =
        serde_json::from_str(&get_text(agent, &url)?).map_err(|e| e.to_string())?;
    let version = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("latest")
        .to_string();
    let assets = json
        .get("assets")
        .and_then(|a| a.as_array())
        .ok_or("The release has no files")?;
    assets
        .iter()
        .find_map(|a| {
            let name = a.get("name")?.as_str()?;
            let url = a.get("browser_download_url")?.as_str()?;
            pick(name).then(|| Download {
                url: url.to_string(),
                file_name: name.to_string(),
                version: version.clone(),
            })
        })
        .ok_or_else(|| format!("No Windows download found in {repo} {version}"))
}

fn find_latest(agent: &ureq::Agent, emulator: &str) -> Result<Download, String> {
    match emulator {
        "DuckStation" => github(agent, "stenzek/duckstation", Some("latest"), |n| {
            n == "duckstation-windows-x64-release.zip"
        }),
        "PCSX2" => github(agent, "PCSX2/pcsx2", None, |n| {
            n.contains("windows-x64-Qt") && n.ends_with(".7z") && !n.contains("symbols")
        }),
        "RPCS3" => github(agent, "RPCS3/rpcs3-binaries-win", None, |n| {
            n.contains("win64") && n.ends_with(".7z")
        }),
        "Flycast" => github(agent, "flyinghead/flycast", None, |n| {
            n.starts_with("flycast-win64") && n.ends_with(".zip")
        }),
        "PPSSPP" => {
            let page = get_text(agent, "https://www.ppsspp.org/download/")?;
            let end = page
                .find("/ppsspp_win.zip")
                .ok_or("PPSSPP's download link wasn't found")?;
            let start = page
                .get(..end)
                .and_then(|s| s.rfind("/files/"))
                .ok_or("PPSSPP's download link wasn't found")?;
            let path = page
                .get(start..end + "/ppsspp_win.zip".len())
                .ok_or("Bad PPSSPP link")?;
            let version = path
                .trim_start_matches("/files/")
                .split('/')
                .next()
                .unwrap_or("")
                .replace('_', ".");
            Ok(Download {
                url: format!("https://www.ppsspp.org{path}"),
                file_name: "ppsspp_win.zip".into(),
                version,
            })
        }
        "RetroArch" => {
            let listing = get_text(agent, "https://buildbot.libretro.com/stable/")?;
            let version = listing
                .split(['"', '>', '<', '/'])
                .filter_map(|s| {
                    let parts: Vec<u32> = s
                        .split('.')
                        .map(|p| p.parse().ok())
                        .collect::<Option<_>>()?;
                    (parts.len() == 3).then_some((parts, s.to_string()))
                })
                .max_by(|a, b| a.0.cmp(&b.0))
                .map(|(_, s)| s)
                .ok_or("No RetroArch release found")?;
            Ok(Download {
                url: format!(
                    "https://buildbot.libretro.com/stable/{version}/windows/x86_64/RetroArch.7z"
                ),
                file_name: "RetroArch.7z".into(),
                version,
            })
        }
        _ => Err(format!(
            "GameShelf can't install {emulator} by itself. Use its Download button."
        )),
    }
}

/// Downloads, unpacks and finds the program. Runs on a background thread.
pub fn install(emulator: &'static str, root: PathBuf, tx: Sender<Msg>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let send = |m: Msg| {
            let _ = tx.send(m);
            ctx.request_repaint();
        };
        let result = run(emulator, &root, &send);
        send(Msg::Done(result));
    });
}

fn run(emulator: &str, root: &Path, send: &dyn Fn(Msg)) -> Result<PathBuf, String> {
    let emulators = root.join(workspace::EMULATORS);
    let target = emulators.join(emulator);
    if target.exists() {
        return Err(format!(
            "{} already exists. Use Choose .exe, or remove that folder first.",
            target.display()
        ));
    }
    let agent = agent();
    send(Msg::Status(format!("Looking for the latest {emulator}…")));
    let dl = find_latest(&agent, emulator)?;

    let work = emulators.join(".gameshelf-download");
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let archive = work.join(&dl.file_name);
    send(Msg::Status(format!(
        "Downloading {emulator} {}…",
        dl.version
    )));
    download(&agent, &dl.url, &archive, send)?;

    send(Msg::Status(format!("Unpacking {emulator}…")));
    let unpacked = work.join(format!("{emulator}-unpacked"));
    let _ = fs::remove_dir_all(&unpacked);
    fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
    let extracted = extract(&archive, &unpacked);
    let _ = fs::remove_file(&archive);
    extracted?;

    // Some archives hold one top folder; use what's inside it.
    let entries: Vec<PathBuf> = fs::read_dir(&unpacked)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .collect();
    let content = match entries.as_slice() {
        [only] if only.is_dir() => only.clone(),
        _ => unpacked.clone(),
    };
    fs::rename(&content, &target)
        .map_err(|e| format!("Could not move {emulator} into place: {e}"))?;
    let _ = fs::remove_dir_all(&work);

    let exe = workspace::find_emulators(&target)
        .into_iter()
        .find(|(name, _)| *name == emulator)
        .map(|(_, exe)| exe)
        .ok_or_else(|| {
            format!(
                "{emulator} was unpacked to {}, but its program wasn't found",
                target.display()
            )
        })?;
    Ok(exe)
}

fn download(agent: &ureq::Agent, url: &str, to: &Path, send: &dyn Fn(Msg)) -> Result<(), String> {
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());
    if total.is_some_and(|t| t > MAX_DOWNLOAD) {
        return Err("The download is unexpectedly large; stopped.".into());
    }
    let part = to.with_extension("part");
    let mut file = fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut reader = resp.into_reader().take(MAX_DOWNLOAD);
    let mut buf = vec![0u8; 256 * 1024];
    let mut done: u64 = 0;
    let mut last_report = 0;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(buf.get(..n).unwrap_or_default())
            .map_err(|e| e.to_string())?;
        done += n as u64;
        if done - last_report >= 1024 * 1024 {
            last_report = done;
            send(Msg::Progress { done, total });
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    if total.is_some_and(|t| t != done) {
        let _ = fs::remove_file(&part);
        return Err("The download was cut short. Please try again.".into());
    }
    fs::rename(&part, to).map_err(|e| e.to_string())
}

/// Unpacks .zip and .7z with Windows' own `tar` (libarchive).
fn extract(archive: &Path, into: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new("tar");
    cmd.arg("-xf").arg(archive).arg("-C").arg(into);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("Could not run tar: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Could not unpack: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpacks_and_flattens_a_single_top_folder() {
        let base = std::env::temp_dir().join(format!("gameshelf-install-{}", std::process::id()));
        let src = base.join("src").join("flycast-win64");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("flycast.exe"), b"x").unwrap();
        let archive = base.join("flycast.zip");
        let status = std::process::Command::new("tar")
            .current_dir(base.join("src"))
            .args(["-a", "-cf"])
            .arg(&archive)
            .arg("flycast-win64")
            .status()
            .unwrap();
        assert!(status.success());
        let out = base.join("out");
        fs::create_dir_all(&out).unwrap();
        extract(&archive, &out).unwrap();
        assert!(out.join("flycast-win64/flycast.exe").is_file());
        fs::remove_dir_all(&base).unwrap();
    }

    /// Looks up (doesn't download) the latest build of every emulator.
    /// Needs the internet: `cargo test -- --ignored finds_every_download`.
    #[test]
    #[ignore]
    fn finds_every_download() {
        let agent = agent();
        for emu in [
            "RetroArch",
            "DuckStation",
            "PCSX2",
            "PPSSPP",
            "RPCS3",
            "Flycast",
        ] {
            let dl = find_latest(&agent, emu).unwrap();
            println!("{emu:12} {:24} {}", dl.version, dl.url);
            assert!(dl.url.starts_with("https://"));
            assert!(dl.file_name.ends_with(".zip") || dl.file_name.ends_with(".7z"));
        }
    }

    #[test]
    fn only_known_emulators_install() {
        assert!(can_install("PCSX2"));
        assert!(!can_install("Dolphin"));
    }
}
