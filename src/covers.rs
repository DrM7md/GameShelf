//! Box art: downloaded from libretro-thumbnails or picked by the user,
//! cached on disk and decoded on a background thread.

use crate::platform::Platform;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const COVER_WIDTH: u32 = 360;

pub enum Job {
    /// Decode a cover already on disk.
    Load { key: PathBuf, file: PathBuf },
    /// Look the game up online and save what's found to `save_to`.
    Fetch {
        key: PathBuf,
        platform: Platform,
        title: String,
        save_to: PathBuf,
    },
    /// An image from a known address, kept in `save_to` so it's fetched only once
    /// (Free games covers).
    Url {
        key: PathBuf,
        url: String,
        save_to: PathBuf,
    },
}

impl Job {
    fn key(&self) -> &Path {
        match self {
            Job::Load { key, .. } | Job::Fetch { key, .. } | Job::Url { key, .. } => key,
        }
    }
}

enum Done {
    Ready {
        key: PathBuf,
        fetched: Option<PathBuf>,
        image: ColorImage,
    },
    Missing {
        key: PathBuf,
        fetched: bool,
    },
}

pub enum Cover {
    Pending,
    Missing,
    Ready(TextureHandle),
}

/// What the library has to remember after a download.
pub enum Update {
    Found { key: PathBuf, file: PathBuf },
    NotFound { key: PathBuf },
}

pub struct Covers {
    jobs: Sender<Job>,
    done: Receiver<Done>,
    state: HashMap<PathBuf, Cover>,
}

impl Covers {
    pub fn new(ctx: &egui::Context) -> Self {
        let (jobs, job_rx) = channel::<Job>();
        let (done_tx, done) = channel::<Done>();
        let ctx = ctx.clone();
        thread::spawn(move || worker(job_rx, done_tx, ctx));
        Self {
            jobs,
            done,
            state: HashMap::new(),
        }
    }

    pub fn request(&mut self, job: Job) {
        self.state.insert(job.key().to_path_buf(), Cover::Pending);
        if self.jobs.send(job).is_err() {
            // The worker is gone; nothing will arrive.
        }
    }

    pub fn get(&self, key: &Path) -> Option<&Cover> {
        self.state.get(key)
    }

    pub fn forget(&mut self, key: &Path) {
        self.state.remove(key);
    }

    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<Update> {
        let mut updates = Vec::new();
        while let Ok(done) = self.done.try_recv() {
            match done {
                Done::Ready {
                    key,
                    fetched,
                    image,
                } => {
                    let texture =
                        ctx.load_texture(key.display().to_string(), image, TextureOptions::LINEAR);
                    self.state.insert(key.clone(), Cover::Ready(texture));
                    if let Some(file) = fetched {
                        updates.push(Update::Found { key, file });
                    }
                }
                Done::Missing { key, fetched } => {
                    self.state.insert(key.clone(), Cover::Missing);
                    if fetched {
                        updates.push(Update::NotFound { key });
                    }
                }
            }
        }
        updates
    }
}

/// A stable file name for a game's cover.
pub fn cover_file(dir: &Path, key: &Path, suffix: &str) -> PathBuf {
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    dir.join(format!("{:016x}{suffix}", h.finish()))
}

fn worker(jobs: Receiver<Job>, done: Sender<Done>, ctx: egui::Context) {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent("GameShelf")
        .build();
    for job in jobs {
        let result = match job {
            Job::Load { key, file } => match decode_file(&file) {
                Some(image) => Done::Ready {
                    key,
                    fetched: None,
                    image,
                },
                None => Done::Missing {
                    key,
                    fetched: false,
                },
            },
            Job::Fetch {
                key,
                platform,
                title,
                save_to,
            } => {
                let found = download(&agent, platform, &title).and_then(|bytes| {
                    let image = decode(&bytes)?;
                    if let Some(dir) = save_to.parent() {
                        std::fs::create_dir_all(dir).ok()?;
                    }
                    std::fs::write(&save_to, &bytes).ok()?;
                    Some(image)
                });
                match found {
                    Some(image) => Done::Ready {
                        key,
                        fetched: Some(save_to),
                        image,
                    },
                    None => Done::Missing { key, fetched: true },
                }
            }
            Job::Url { key, url, save_to } => {
                let cached = decode_file(&save_to);
                let image = cached.or_else(|| {
                    let resp = agent.get(&url).call().ok()?;
                    let mut bytes = Vec::new();
                    resp.into_reader()
                        .take(MAX_IMAGE_BYTES)
                        .read_to_end(&mut bytes)
                        .ok()?;
                    let image = decode(&bytes)?;
                    if let Some(dir) = save_to.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = std::fs::write(&save_to, &bytes);
                    Some(image)
                });
                match image {
                    Some(image) => Done::Ready {
                        key,
                        fetched: None,
                        image,
                    },
                    None => Done::Missing {
                        key,
                        fetched: false,
                    },
                }
            }
        };
        if done.send(result).is_err() {
            break;
        }
        ctx.request_repaint();
    }
}

fn decode_file(file: &Path) -> Option<ColorImage> {
    let len = std::fs::metadata(file).ok()?.len();
    if len > MAX_IMAGE_BYTES {
        return None;
    }
    decode(&std::fs::read(file).ok()?)
}

fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let mut img = image::load_from_memory(bytes).ok()?;
    if img.width() > COVER_WIDTH {
        img = img.resize(
            COVER_WIDTH,
            COVER_WIDTH.saturating_mul(3),
            image::imageops::FilterType::Triangle,
        );
    }
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

/// libretro-thumbnails folder name for each system.
fn system_folder(platform: Platform) -> Option<&'static str> {
    Some(match platform {
        Platform::Ps1 => "Sony - PlayStation",
        Platform::Ps2 => "Sony - PlayStation 2",
        Platform::Ps3 => "Sony - PlayStation 3",
        Platform::Psp => "Sony - PlayStation Portable",
        Platform::GameCube => "Nintendo - GameCube",
        Platform::Nes => "Nintendo - Nintendo Entertainment System",
        Platform::Snes => "Nintendo - Super Nintendo Entertainment System",
        Platform::N64 => "Nintendo - Nintendo 64",
        Platform::Gb => "Nintendo - Game Boy",
        Platform::Gbc => "Nintendo - Game Boy Color",
        Platform::Gba => "Nintendo - Game Boy Advance",
        Platform::Nds => "Nintendo - Nintendo DS",
        Platform::Genesis => "Sega - Mega Drive - Genesis",
        Platform::Dreamcast => "Sega - Dreamcast",
        Platform::Unknown => return None,
    })
}

/// Tries the title as-is, then with the common region tags.
fn download(agent: &ureq::Agent, platform: Platform, title: &str) -> Option<Vec<u8>> {
    let folder = system_folder(platform)?;
    // libretro replaces these characters with '_' in file names.
    let name: String = title
        .chars()
        .map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c })
        .collect();
    let mut names = vec![name.clone()];
    if !name.contains('(') {
        for region in ["(USA)", "(USA, Europe)", "(Europe)", "(World)", "(Japan)"] {
            names.push(format!("{name} {region}"));
        }
    }
    for candidate in names {
        let url = format!(
            "https://thumbnails.libretro.com/{}/Named_Boxarts/{}.png",
            encode(folder),
            encode(&candidate)
        );
        let Ok(resp) = agent.get(&url).call() else {
            continue;
        };
        let mut bytes = Vec::new();
        if resp
            .into_reader()
            .take(MAX_IMAGE_BYTES)
            .read_to_end(&mut bytes)
            .is_ok()
            && bytes.starts_with(&[0x89, b'P', b'N', b'G'])
        {
            return Some(bytes);
        }
    }
    None
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
