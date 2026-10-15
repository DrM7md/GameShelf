// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod controls;
mod covers;
mod library;
mod organize;
mod platform;
mod theme;
mod workspace;

use eframe::egui;

fn main() -> eframe::Result {
    // `gameshelf --organize`: the "Organize my games" button, without the window.
    if std::env::args().any(|a| a == "--organize") {
        organize_from_command_line();
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
