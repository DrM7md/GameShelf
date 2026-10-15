// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod controls;
mod covers;
mod emukeys;
mod freegames;
mod install;
mod library;
mod organize;
mod platform;
mod saves;
mod theme;
mod workspace;

use eframe::egui;

fn main() -> eframe::Result {
    // `gameshelf --organize`: the "Organize my games" button, without the window.
    if std::env::args().any(|a| a == "--organize") {
        organize_from_command_line();
        return Ok(());
    }
    // `gameshelf --install DuckStation`: the Install button, without the window.
    let args: Vec<String> = std::env::args().collect();
    if let Some(name) = args
        .iter()
        .position(|a| a == "--install")
        .and_then(|i| args.get(i + 1))
    {
        install_from_command_line(name);
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GameShelf")
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([720.0, 460.0]),
        ..Default::default()
    };
    eframe::run_native(
        "GameShelf",
        options,
        Box::new(|cc| Ok(Box::new(app::GameShelfApp::new(cc)))),
    )
}

fn install_from_command_line(name: &str) {
    let (mut lib, _) = library::Library::load();
    let Some(root) = lib.root.clone().or_else(workspace::default_root) else {
        println!("No GameShelf folder.");
        return;
    };
    if let Err(e) = workspace::create(&root) {
        println!("Could not create the GameShelf folder: {e}");
        return;
    }
    let mut last_mb = 0;
    let report = |msg: install::Msg| match msg {
        install::Msg::Status(s) => println!("{s}"),
        install::Msg::Progress { done, total } => {
            let mb = done / (1024 * 1024);
            // One line every 5 MB is plenty.
            if mb >= last_mb + 5 || total == Some(done) {
                last_mb = mb;
                match total {
                    Some(t) => println!("  {mb} / {} MB", t / (1024 * 1024)),
                    None => println!("  {mb} MB"),
                }
            }
        }
        install::Msg::Done(_) => {}
    };
    let mut report = report;
    match install::install_now(name, &root, &mut report) {
        Ok(exe) => {
            for p in platform::ALL {
                if p.emulator().is_some_and(|i| i.name == name) {
                    lib.emulators.entry(p).or_default().exe = exe.display().to_string();
                }
            }
            lib.root.get_or_insert(root);
            match lib.save() {
                Ok(()) => println!("Installed: {}", exe.display()),
                Err(e) => println!("Installed, but the library couldn't be saved: {e}"),
            }
        }
        Err(e) => println!("Could not install {name}: {e}"),
    }
}

fn organize_from_command_line() {
    let (mut lib, _) = library::Library::load();
    let Some(root) = lib.root.clone() else {
        println!("No GameShelf folder set. Choose one in Settings first.");
        return;
    };
    if let Err(e) = workspace::create(&root) {
        println!("Could not create the folders: {e}");
        return;
    }
    let plan = organize::plan(&lib, &root);
    println!(
        "Plan: {} to move, {} already in place, {} without a system, {} missing, {} conflicts",
        plan.moves.len(),
        plan.in_place,
        plan.unknown,
        plan.missing,
        plan.conflicts.len()
    );
    for m in &plan.moves {
        match organize::apply(m) {
            Ok(()) => {
                println!("  moved  {} -> {}", m.title, m.new_path.display());
                if let Some(g) = lib.games.iter_mut().find(|g| g.path == m.old_path) {
                    g.path = m.new_path.clone();
                }
            }
            Err(e) => println!("  FAILED {}: {e}", m.title),
        }
    }
    match lib.save() {
        Ok(()) => println!("Library saved."),
        Err(e) => println!("Could not save the library: {e}"),
    }
}
