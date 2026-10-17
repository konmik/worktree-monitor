#[path = "features/window/design/all-text-shares-one-monospace-font-and-size/resources.rs"]
mod font_resources;

#[path = "features/window/design/layout-stays-compact-when-the-window-narrows/native.rs"]
mod row_layout;

#[path = "features/window/repo-list/worktree/indicators/source-count-indicators-stay-visible-and-highlight-nonzero-changes/native.rs"]
mod count_indicators;

#[path = "features/window/repo-list/worktree/indicators/nonzero-line-change-indicators-stay-fully-opaque/native.rs"]
mod opaque_indicators;

#[path = "features/window/repo-list/refresh-markers-identify-only-the-active-target/native.rs"]
mod refresh_markers;

#[path = "features/window/idle-indicator/idle-percentage-shows-unused-monitor-time-over-the-last-minute/native.rs"]
mod idle_indicator;

#[path = "features/window/visibility-controls/visibility-controls-appear-only-while-the-mouse-hovers-over-the-window/native.rs"]
mod visibility_controls;

#[path = "features/window/the-window-fades-outside-hover/native.rs"]
mod window_fading;

#[path = "features/window/users-can-move-the-window/native.rs"]
mod window_movement;

#[path = "features/context-menu/users-can-open-the-context-menu/native.rs"]
mod context_menu_actions;

#[path = "features/window/design/the-dashboard-has-a-flat-terminal-appearance/native.rs"]
mod window_appearance;

#[cfg(test)]
#[path = "features/window/repo-list/worktree/indicators/source-count-indicators-stay-visible-and-highlight-nonzero-changes/native.test.rs"]
mod count_indicator_tests;

#[cfg(test)]
#[path = "features/window/repo-list/worktree/indicators/nonzero-line-change-indicators-stay-fully-opaque/native.test.rs"]
mod opaque_indicator_tests;

#[cfg(test)]
#[path = "features/window/repo-list/refresh-markers-identify-only-the-active-target/native.test.rs"]
mod refresh_marker_tests;

#[cfg(test)]
#[path = "features/window/design/layout-stays-compact-when-the-window-narrows/native.test.rs"]
mod row_layout_tests;

#[path = "features/window/repo-list/users-can-view-repository-and-worktree-status/native.rs"]
mod dashboard_rows;

#[path = "features/window/repo-list/users-can-reorder-rows/native.rs"]
mod row_ordering;

#[path = "features/window/repo-list/worktree/users-can-hide-and-restore-worktrees/native.rs"]
mod worktree_visibility;

#[path = "features/window/repo-list/users-can-scroll-overflowing-rows/native.rs"]
mod row_scrolling;

#[path = "features/window/repo-list/the-visible-row-survives-refresh-and-layout-changes/native.rs"]
mod scroll_anchor;

#[path = "features/window/users-can-view-the-live-dashboard/native.rs"]
mod runtime;
pub use runtime::run;

#[path = "features/window/users-can-resize-the-window/native.rs"]
mod window_resizing;

#[path = "features/window/saved-preferences-reload-while-the-dashboard-is-open/native.rs"]
mod saved_preferences;

#[path = "features/window/repo-list/worktree/indicators/loading-reserves-count-space-without-placeholder-messages/native.rs"]
mod loading_rows;

#[path = "features/notification-icon/the-app-has-one-notification-icon/native.rs"]
mod notification_lifecycle;

#[path = "features/window/window-position-survives-restart/native.rs"]
mod window_placement;

#[path = "features/window/repo-list/users-can-reorder-rows/users-can-reorder-repositories/native.rs"]
mod repository_ordering;

#[path = "features/window/repo-list/users-can-reorder-rows/users-can-reorder-worktrees/native.rs"]
mod worktree_ordering;

#[path = "features/window/preference-failures-are-visible-without-losing-current-settings/native.rs"]
mod preference_failures;

#[path = "features/window/the-window-starts-at-its-intended-size/native.rs"]
mod initial_geometry;

#[path = "features/window/dashboard-opens-above-other-windows-without-taking-focus/native.rs"]
mod dashboard_opening;

#[path = "features/window/closing-the-dashboard-stops-its-monitor/native.rs"]
mod dashboard_closing;

#[path = "features/window/repo-list/worktree/indicators/count-indicators-hide-empty-optional-groups/native.rs"]
mod optional_count_indicators;
