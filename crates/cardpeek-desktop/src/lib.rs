//! Dioxus desktop frontend and its application-state adapter.

mod backend;
pub mod controller;
pub mod state;
pub mod ui;

pub use backend::{DesktopBackend, ReplayBackend};
pub use controller::{Controller, UiMessage};
pub use ui::App;
