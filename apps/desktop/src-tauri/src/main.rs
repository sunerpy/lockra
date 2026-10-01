//! Desktop entry point.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lockra_desktop_lib::run();
}
