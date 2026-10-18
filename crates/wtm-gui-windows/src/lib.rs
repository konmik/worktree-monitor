

#[path = "features/context-menu/users-can-select-opacity-in-five-percent-steps/opacity.rs"]
pub mod opacity;
#[path = "features/window/window-position-survives-restart/placement.rs"]
pub mod placement;

#[cfg(any(windows, test))]
#[path = "features/window/design/all-text-shares-one-monospace-font-and-size/typography.rs"]
mod typography;

#[cfg(windows)]
#[path = "features/window/window-position-survives-restart/placement_win.rs"]
mod placement_win;

#[cfg(windows)]
#[path = "features/notification-icon/the-app-has-one-notification-icon/icon.rs"]
mod notification_icon;

#[cfg(windows)]
#[path = "features/window/the-app-has-no-taskbar-icon/style.rs"]
mod taskbar_icon;

#[cfg(windows)]
#[path = "features/context-menu/users-can-open-the-context-menu/menu.rs"]
mod context_menu;

#[cfg(windows)]
mod window;

#[cfg(windows)]
#[path = "features/gui-startup/dashboard-starts-with-user-service/startup.rs"]
mod startup;

#[cfg(windows)]
pub use startup::run;

#[path = "features/window/users-can-view-the-live-dashboard/geometry.rs"]
mod layout;
