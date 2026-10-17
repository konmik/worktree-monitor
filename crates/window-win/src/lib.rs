#[path = "features/window/repo-list/worktree/indicators/loading-reserves-count-space-without-placeholder-messages/spacing.rs"]
mod loading_rows;

#[path = "features/window/repo-list/worktree/indicators/source-count-indicators-stay-visible-and-highlight-nonzero-changes/counts.rs"]
mod count_indicators;
#[path = "features/window/repo-list/worktree/indicators/count-indicators-hide-empty-optional-groups/counts.rs"]
mod optional_count_indicators;
#[path = "features/window/repo-list/worktree/indicators/nonzero-line-change-indicators-stay-fully-opaque/opacity.rs"]
mod count_opacity;

#[path = "features/window/repo-list/users-can-view-repository-and-worktree-status/rendering.rs"]
pub mod rendering;

#[path = "features/render-buffer/application-renders-into-a-render-buffer/buffer.rs"]
pub mod render_buffer;

#[path = "features/window/repo-list/users-can-view-repository-and-worktree-status/dashboard.rs"]
pub mod dashboard;
#[path = "features/window/repo-list/users-can-reorder-rows/order.rs"]
pub mod order;
#[path = "features/context-menu/users-can-select-opacity-in-five-percent-steps/opacity.rs"]
pub mod opacity;
#[path = "features/window/window-position-survives-restart/placement.rs"]
pub mod placement;
#[path = "features/window/repo-list/worktree/users-can-hide-and-restore-worktrees/visibility.rs"]
pub mod visibility;

#[path = "features/window/design/layout-stays-compact-when-the-window-narrows/layout.rs"]
mod layout;

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
pub use window::run;

#[path = "features/window/design/layout-stays-compact-when-the-window-narrows/name-fitting.rs"]
mod name_fitting;

#[path = "features/window/repo-list/users-can-view-repository-and-worktree-status/scan.rs"]
mod scan_projection;

#[path = "features/window/design/layout-stays-compact-when-the-window-narrows/wrapping.rs"]
mod wrapping;

#[path = "features/window/preference-failures-are-visible-without-losing-current-settings/projection.rs"]
mod preference_failures;

#[path = "features/window/repo-list/worktree/users-can-hide-and-restore-worktrees/projection.rs"]
mod visibility_projection;
