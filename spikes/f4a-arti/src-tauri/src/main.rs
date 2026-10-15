// Niente finestra console su Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() == 4 && a[1] == "--bench" {
        return spike_f4a_lib::bench(a[2].trim_end_matches('/').to_string(), a[3].clone().into());
    }
    spike_f4a_lib::run()
}
