#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Mode FILL de l'Autonomous Jam: el binari es relanza com a agent
    // supervisat (RLIMIT_AS, process group) perquè l'agent pugui treballar
    // hores sense que el Mac es pengi. Detecta'l ABANS d'arrencar Tauri.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--jam-agent") {
        std::process::exit(no_orbit_lib::jam::agent::run_agent_mode(&args));
    }
    no_orbit_lib::run()
}
