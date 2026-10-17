pub mod dashboard;
pub mod order;
#[path = "features/context-menu/users-can-open-the-context-menu/users-can-select-opacity-in-five-percent-steps/opacity.rs"]
pub mod opacity;
#[path = "features/window/window-position-survives-restart/placement.rs"]
pub mod placement;
pub mod visibility;

#[cfg(any(windows, test))]
#[path = "features/window/design/layout-stays-compact-when-the-window-narrows/layout.rs"]
mod layout;

#[cfg(any(windows, test))]
#[path = "features/window/design/all-text-shares-one-monospace-font-and-size/typography.rs"]
mod typography;

#[cfg(windows)]
#[path = "features/window/window-position-survives-restart/placement_win.rs"]
mod placement_win;

#[cfg(windows)]
#[path = "features/notification-icon/the-app-has-one-notification-icon-and-no-taskbar-icon/icon.rs"]
mod notification_icon;

#[cfg(windows)]
#[path = "features/context-menu/users-can-open-the-context-menu/menu.rs"]
mod context_menu;

#[cfg(windows)]
mod native;

#[cfg(windows)]
pub use native::run;

#[cfg(not(windows))]
pub fn run(_service: wtm_core::Service, _launch_directory: std::path::PathBuf) -> Result<(), std::io::Error> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "--live is supported only on Windows"))
}
