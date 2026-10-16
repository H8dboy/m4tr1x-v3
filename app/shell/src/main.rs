// Niente finestra di console accanto all'app nella versione distribuita (Windows).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    m4tr1x_shell_lib::run();
}
