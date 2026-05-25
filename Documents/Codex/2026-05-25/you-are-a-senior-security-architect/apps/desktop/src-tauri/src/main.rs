//! ESPASS desktop application entry point.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    espass_desktop::run();
}
