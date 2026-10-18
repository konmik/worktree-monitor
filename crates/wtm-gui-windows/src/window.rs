#[path = "features/render-buffer/render-buffer-is-handed-to-the-system-specific-window-implementation/presentation.rs"]
mod buffer_presentation;

#[path = "features/render-buffer/render-buffer-is-handed-to-the-system-specific-window-implementation/fonts.rs"]
mod cell_fonts;

#[path = "features/window/design/all-text-shares-one-monospace-font-and-size/resources.rs"]
mod font_resources;

#[path = "features/window/users-can-view-the-live-dashboard/row-layout.rs"]
mod row_layout;

#[path = "features/render-buffer/stressed-cells-render-at-full-opacity/overlay.rs"]
mod opaque_indicators;

#[path = "features/window/users-can-view-the-live-dashboard/markers.rs"]
mod refresh_markers;

#[path = "features/window/users-can-view-the-live-dashboard/status.rs"]
mod idle_indicator;

#[path = "features/window/visibility-controls/visibility-controls-appear-only-while-the-mouse-hovers-over-the-window/controls.rs"]
mod visibility_controls;

#[path = "features/window/the-window-fades-outside-hover/hover.rs"]
mod window_fading;

#[path = "features/window/users-can-move-the-window/movement.rs"]
mod window_movement;

#[path = "features/context-menu/users-can-open-the-context-menu/actions.rs"]
mod context_menu_actions;

#[path = "features/window/design/the-dashboard-has-a-flat-terminal-appearance/painting.rs"]
mod window_appearance;


#[cfg(test)]
#[path = "features/render-buffer/render-buffer-is-handed-to-the-system-specific-window-implementation/opacity.test.rs"]
mod opaque_indicator_tests;

#[cfg(test)]
#[path = "features/render-buffer/render-buffer-is-handed-to-the-system-specific-window-implementation/markers.test.rs"]
mod refresh_marker_tests;

#[cfg(test)]
#[path = "features/window/users-can-view-the-live-dashboard/row-layout.test.rs"]
mod row_layout_tests;

#[path = "features/window/users-can-view-the-live-dashboard/rows.rs"]
mod dashboard_rows;

#[path = "features/window/repo-list/users-can-reorder-rows/dragging.rs"]
mod row_ordering;

#[path = "features/window/repo-list/worktree/users-can-hide-and-restore-worktrees/controls.rs"]
mod worktree_visibility;

#[path = "features/window/repo-list/users-can-scroll-overflowing-rows/scrolling.rs"]
mod row_scrolling;

#[path = "features/window/repo-list/the-visible-row-survives-refresh-and-layout-changes/anchor.rs"]
mod scroll_anchor;

#[path = "features/window/users-can-view-the-live-dashboard/runtime.rs"]
mod runtime;
pub use runtime::run;

#[path = "features/window/users-can-resize-the-window/resizing.rs"]
mod window_resizing;

#[path = "features/window/saved-preferences-reload-while-the-dashboard-is-open/reload.rs"]
mod saved_preferences;


#[path = "features/notification-icon/the-app-has-one-notification-icon/lifecycle.rs"]
mod notification_lifecycle;

#[path = "features/window/window-position-survives-restart/restoration.rs"]
mod window_placement;

#[path = "features/window/repo-list/users-can-reorder-rows/users-can-reorder-repositories/drop-target.rs"]
mod repository_ordering;

#[path = "features/window/repo-list/users-can-reorder-rows/users-can-reorder-worktrees/drop-target.rs"]
mod worktree_ordering;

#[path = "features/window/users-can-view-the-live-dashboard/preference-persistence.rs"]
mod preference_failures;

#[path = "features/window/the-window-starts-at-its-intended-size/geometry.rs"]
mod initial_geometry;

#[path = "features/window/dashboard-opens-above-other-windows-without-taking-focus/opening.rs"]
mod dashboard_opening;

#[path = "features/window/closing-the-dashboard-stops-its-monitor/shutdown.rs"]
mod dashboard_closing;
