use crate::state::AppState;
use crate::theme;

pub fn message_list(ui: &mut egui::Ui, state: &mut AppState) {
    if state.messages.is_empty() {
        ui.label(
            egui::RichText::new("No file loaded")
                .italics()
                .color(theme::TEXT_DIM),
        );
        return;
    }

    // Compact scrollable list
    let row_height = 22.0;
    let max_visible = 15;
    let height = (state.messages.len().min(max_visible) as f32) * row_height + 4.0;

    // Build display data first (avoid borrow conflict)
    let items: Vec<(usize, String, bool)> = state
        .messages
        .iter()
        .enumerate()
        .map(|(i, msg)| {
            let selected = state.selected_msg == Some(i);
            let label = format!("{:3} | {} @ {}", i, msg.name, msg.level);
            (i, label, selected)
        })
        .collect();

    let mut clicked_idx: Option<usize> = None;

    egui::ScrollArea::vertical()
        .max_height(height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (i, label, selected) in &items {
                let text = egui::RichText::new(label)
                    .small()
                    .color(if *selected {
                        egui::Color32::WHITE
                    } else {
                        theme::TEXT_DIM
                    });

                let btn = egui::Button::new(text)
                    .fill(if *selected {
                        theme::ACCENT.linear_multiply(0.2)
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .frame(false)
                    .min_size(egui::vec2(ui.available_width(), row_height));

                let response = ui.add(btn);
                if response.clicked() {
                    clicked_idx = Some(*i);
                }
                if response.hovered() && !selected {
                    ui.painter().rect_filled(
                        response.rect,
                        2.0,
                        egui::Color32::from_white_alpha(8),
                    );
                }
            }
        });

    // Handle click outside the borrow
    if let Some(idx) = clicked_idx {
        state.select_message(idx);
        state.auto_colormap();
    }

    // Info about selected message
    if let Some(idx) = state.selected_msg {
        if idx < state.messages.len() {
            let name = state.messages[idx].name.clone();
            let units = state.messages[idx].units.clone();
            let level = state.messages[idx].level.clone();
            let fhr = state.messages[idx].forecast_hr;
            let nx = state.messages[idx].nx;
            let ny = state.messages[idx].ny;
            ui.add_space(4.0);
            ui.group(|ui| {
                ui.label(
                    egui::RichText::new(&name)
                        .strong()
                        .color(theme::ACCENT),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{} | {} | F{:03} | {}x{}",
                        units, level, fhr, nx, ny
                    ))
                    .small()
                    .color(theme::TEXT_DIM),
                );
            });
        }
    }
}
