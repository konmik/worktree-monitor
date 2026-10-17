#[path = "features/window/repo-list/users-can-view-repository-and-worktree-status/dashboard.rs"]
pub mod dashboard;
#[path = "features/window/repo-list/users-can-reorder-rows/order.rs"]
pub mod order;
#[path = "features/context-menu/users-can-open-the-context-menu/users-can-select-opacity-in-five-percent-steps/opacity.rs"]
pub mod opacity;
#[path = "features/window/window-position-survives-restart/placement.rs"]
pub mod placement;
#[path = "features/window/repo-list/worktree/users-can-hide-and-restore-worktrees/visibility.rs"]
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

#[cfg(any(windows, test))]
#[path = "features/window/design/layout-stays-compact-when-the-window-narrows/name-fitting.rs"]
mod name_fitting;

#[cfg(any(windows, test))]
#[path = "features/window/design/layout-stays-compact-when-the-window-narrows/spacing.rs"]
mod spacing;

#[path = "features/window/repo-list/users-can-view-repository-and-worktree-status/scan.rs"]
mod scan_projection;

#[cfg(any(windows, test))]
#[path = "features/window/design/layout-stays-compact-when-the-window-narrows/wrapping.rs"]
mod wrapping;

#[path = "features/window/preference-failures-are-visible-without-losing-current-settings/projection.rs"]
mod preference_failures;

#[path = "features/window/repo-list/worktree/users-can-hide-and-restore-worktrees/projection.rs"]
mod visibility_projection;
