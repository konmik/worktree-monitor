#[path = "features/dashboard/repo-list/worktree/indicators/loading-reserves-count-space-without-placeholder-messages/spacing.rs"]
mod loading_rows;

#[path = "features/dashboard/repo-list/worktree/indicators/source-count-indicators-stay-visible-and-highlight-nonzero-changes/counts.rs"]
mod count_indicators;

#[path = "features/dashboard/repo-list/worktree/indicators/count-indicators-hide-empty-optional-groups/counts.rs"]
mod optional_count_indicators;

#[path = "features/dashboard/repo-list/worktree/indicators/nonzero-line-change-indicators-stay-fully-opaque/opacity.rs"]
mod count_opacity;

#[path = "features/dashboard/repo-list/users-can-view-repository-and-worktree-status/rendering.rs"]
pub mod rendering;

#[path = "features/render-buffer/application-renders-into-a-render-buffer/buffer.rs"]
pub mod render_buffer;

#[path = "features/dashboard/repo-list/users-can-view-repository-and-worktree-status/dashboard.rs"]
pub mod dashboard;

#[path = "features/dashboard/repo-list/row-order-survives-restart/order.rs"]
pub mod order;

#[path = "features/dashboard/repo-list/worktree/hidden-worktrees-stay-hidden-after-restart/visibility.rs"]
pub mod visibility;

#[path = "features/dashboard/design/layout-stays-compact-when-the-window-narrows/layout.rs"]
mod layout;

#[path = "features/dashboard/design/layout-stays-compact-when-the-window-narrows/name-fitting.rs"]
mod name_fitting;

#[path = "features/dashboard/design/layout-stays-compact-when-the-window-narrows/wrapping.rs"]
mod wrapping;

#[path = "features/dashboard/repo-list/users-can-view-repository-and-worktree-status/scan.rs"]
mod scan_projection;

#[path = "features/dashboard/preference-failures-are-visible-without-losing-current-settings/projection.rs"]
mod preference_failures;

#[path = "features/dashboard/repo-list/worktree/hidden-worktrees-stay-hidden-after-restart/projection.rs"]
mod visibility_projection;

#[cfg(test)]
#[path = "features/dashboard/repo-list/worktree/indicators/source-count-indicators-stay-visible-and-highlight-nonzero-changes/counts.test.rs"]
mod count_indicator_tests;
