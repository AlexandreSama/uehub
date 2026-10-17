// Évite une fenêtre de console sous Windows en release. Ne pas retirer.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    uehub_desktop_lib::run()
}