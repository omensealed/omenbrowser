use ratatui::layout::Rect;

use crate::tui::{AdminAction, AdminTab};
use crate::tui_format::fit_line_to_width;

pub(crate) fn tab_hitboxes(area: Rect) -> Vec<(Rect, AdminTab)> {
    let mut x = area.x.saturating_add(1);
    let y = area.y.saturating_add(1);
    let mut y = y;
    let left = area.x.saturating_add(1);
    let right = area.right().saturating_sub(1);
    AdminTab::ALL
        .iter()
        .filter_map(|tab| {
            let width = tab_label(*tab, area.width).len() as u16 + 4;
            if x > left && x.saturating_add(width) > right {
                x = left;
                y = y.saturating_add(1);
            }
            let hitbox = Rect::new(x, y, width, 1);
            x = x.saturating_add(width);
            (hitbox.right() <= area.right() && hitbox.bottom() <= area.bottom())
                .then_some((hitbox, *tab))
        })
        .collect()
}

pub(crate) fn tab_label(tab: AdminTab, width: u16) -> &'static str {
    if width < 36 {
        tab.compact_title()
    } else {
        tab.title()
    }
}

pub(crate) fn tab_row_count(width: u16) -> u16 {
    let left = 1u16;
    let right = width.max(12).saturating_sub(1);
    let mut x = left;
    let mut rows = 1u16;
    for tab in AdminTab::ALL {
        let tab_width = tab_label(tab, width).len() as u16 + 4;
        if x > left && x.saturating_add(tab_width) > right {
            x = left;
            rows = rows.saturating_add(1);
        }
        x = x.saturating_add(tab_width);
    }
    rows
}

pub(crate) fn tab_panel_height(width: u16) -> u16 {
    tab_row_count(width).saturating_add(2).max(3)
}

pub(crate) fn list_row_at(area: Rect, row: u16, item_count: usize) -> Option<usize> {
    if row < area.y || row >= area.bottom() {
        return None;
    }
    let row_index = row.saturating_sub(area.y) as usize;
    (row_index < item_count).then_some(row_index)
}

pub(crate) fn inner_rect(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    )
}

pub(crate) fn action_hitboxes(
    area: Rect,
    actions: &[(AdminAction, String)],
) -> Vec<(Rect, AdminAction)> {
    actions
        .iter()
        .enumerate()
        .filter_map(|(index, (action, _))| {
            let y = area.y.saturating_add(index as u16);
            (y < area.bottom()).then_some((Rect::new(area.x, y, area.width, 1), *action))
        })
        .collect()
}

pub(crate) fn action_list_label(label: &str, max_width: usize) -> String {
    fit_line_to_width(&format!("[ {label} ]"), max_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_hitboxes_wrap_on_narrow_viewports() {
        let hitboxes = tab_hitboxes(Rect::new(0, 0, 38, 8));

        assert_eq!(hitboxes.len(), AdminTab::ALL.len());
        assert!(hitboxes.iter().all(|(area, _)| area.right() <= 38));
        assert!(hitboxes.iter().any(|(area, _)| area.y > 1));
        assert!(tab_row_count(38) > 1);
    }

    #[test]
    fn tab_panel_height_keeps_all_wrapped_tabs_clickable() {
        let width = 24;
        let height = tab_panel_height(width);
        let hitboxes = tab_hitboxes(Rect::new(0, 0, width, height));

        assert_eq!(hitboxes.len(), AdminTab::ALL.len());
        assert!(hitboxes.iter().all(|(area, _)| area.bottom() <= height));
        assert!(height > tab_row_count(120));
    }

    #[test]
    fn tab_hitboxes_use_compact_labels_on_very_narrow_viewports() {
        let width = 12;
        let height = tab_panel_height(width);
        let hitboxes = tab_hitboxes(Rect::new(0, 0, width, height));

        assert_eq!(hitboxes.len(), AdminTab::ALL.len());
        assert!(hitboxes.iter().all(|(area, _)| area.right() <= width));
        assert!(hitboxes.iter().all(|(area, _)| area.bottom() <= height));
        assert_eq!(tab_label(AdminTab::Monitoring, width), "Mon");
        assert_eq!(tab_label(AdminTab::Monitoring, 80), "Monitoring");
    }

    #[test]
    fn list_row_at_maps_only_visible_content_rows() {
        let area = Rect::new(5, 10, 20, 4);
        assert_eq!(list_row_at(area, 9, 3), None);
        assert_eq!(list_row_at(area, 10, 3), Some(0));
        assert_eq!(list_row_at(area, 12, 3), Some(2));
        assert_eq!(list_row_at(area, 13, 3), None);
        assert_eq!(list_row_at(area, 10, 0), None);
    }

    #[test]
    fn action_hitboxes_cover_visible_actions() {
        let actions = vec![
            (AdminAction::SaveConfig, "Save Config".to_string()),
            (AdminAction::StartLive, "Start Live".to_string()),
            (AdminAction::StopLive, "Stop Live".to_string()),
        ];
        let hitboxes = action_hitboxes(Rect::new(10, 5, 40, 2), &actions);

        assert_eq!(hitboxes.len(), 2);
        assert_eq!(
            hitboxes[0],
            (Rect::new(10, 5, 40, 1), AdminAction::SaveConfig)
        );
        assert_eq!(
            hitboxes[1],
            (Rect::new(10, 6, 40, 1), AdminAction::StartLive)
        );
    }

    #[test]
    fn action_labels_fit_narrow_widths() {
        let label = action_list_label("Configure TCPServerInterface", 18);

        assert!(label.chars().count() <= 18);
        assert!(label.starts_with("[ "));
        assert!(label.ends_with("..."));

        assert_eq!(action_list_label("Save Config", 40), "[ Save Config ]");
        assert_eq!(action_list_label("Save Config", 2), "..");
    }
}
