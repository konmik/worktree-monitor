pub mod dashboard;
pub mod order;
pub mod opacity;
pub mod placement;
pub mod visibility;

#[cfg(any(windows, test))]
mod layout;

#[cfg(any(windows, test))]
mod typography;

#[cfg(windows)]
mod placement_win;

#[cfg(windows)]
#[path = "features/notification-icon/the-app-has-one-notification-icon-and-no-taskbar-icon/icon.rs"]
mod notification_icon;

#[cfg(windows)]
#[path = "features/notification-icon/users-can-open-the-context-menu/menu.rs"]
mod notification_menu;

#[cfg(windows)]
mod native;

#[cfg(windows)]
pub use native::run;

#[cfg(not(windows))]
pub fn run(_service: wtm_core::Service, _launch_directory: std::path::PathBuf) -> Result<(), std::io::Error> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "--live is supported only on Windows"))
}
