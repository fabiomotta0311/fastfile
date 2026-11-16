//! Tema visual customizado para o FastFile (§0.1, §4 e Issue #13).
//!
//! Implementa paleta escura refinada (estilo Zed/VS Code), cantos arredondados,
//! espaçamento proporcional e tipografia limpa. O tema padrão cru do egui
//! é expressamente substituído.

use egui::{Color32, CornerRadius, FontId, Margin, Stroke, TextStyle, Visuals};

pub const COLOR_BG: Color32 = Color32::from_rgb(24, 24, 27); // #18181b (zinc-900)
pub const COLOR_PANEL: Color32 = Color32::from_rgb(30, 30, 36); // #1e1e24
pub const COLOR_CARD: Color32 = Color32::from_rgb(39, 39, 42); // #27272a (zinc-800)
pub const COLOR_CARD_HOVER: Color32 = Color32::from_rgb(50, 50, 56);
pub const COLOR_BORDER: Color32 = Color32::from_rgb(63, 63, 70); // #3f3f46 (zinc-700)
pub const COLOR_ACCENT: Color32 = Color32::from_rgb(99, 102, 241); // #6366f1 (indigo-500)
pub const COLOR_ACCENT_HOVER: Color32 = Color32::from_rgb(129, 140, 248); // #818cf8
pub const COLOR_TEXT_PRIMARY: Color32 = Color32::from_rgb(244, 244, 245); // #f4f4f5 (zinc-100)
pub const COLOR_TEXT_MUTED: Color32 = Color32::from_rgb(161, 161, 170); // #a1a1aa (zinc-400)
pub const COLOR_SUCCESS: Color32 = Color32::from_rgb(34, 197, 94); // #22c55e (green-500)
pub const COLOR_WARNING: Color32 = Color32::from_rgb(234, 179, 8); // #eab308 (yellow-500)
pub const COLOR_DANGER: Color32 = Color32::from_rgb(239, 68, 68); // #ef4444 (red-500)

/// Configura o tema customizado escuro no contexto do egui.
pub fn apply_custom_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    visuals.override_text_color = Some(COLOR_TEXT_PRIMARY);
    visuals.panel_fill = COLOR_PANEL;
    visuals.window_fill = COLOR_PANEL;
    visuals.extreme_bg_color = COLOR_BG;
    visuals.faint_bg_color = COLOR_CARD;

    visuals.window_stroke = Stroke::new(1.0_f32, COLOR_BORDER);
    visuals.window_corner_radius = CornerRadius::same(8);

    // Widgets inativos
    visuals.widgets.inactive.bg_fill = COLOR_CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, COLOR_BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, COLOR_TEXT_PRIMARY);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);

    // Widgets hovered
    visuals.widgets.hovered.bg_fill = COLOR_CARD_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, COLOR_ACCENT_HOVER);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);

    // Widgets ativos / clicados
    visuals.widgets.active.bg_fill = COLOR_ACCENT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, COLOR_ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);

    // Seleção de itens e caixas de texto
    visuals.selection.bg_fill = COLOR_ACCENT.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0_f32, COLOR_ACCENT);

    ctx.set_visuals(visuals);

    // Espaçamento confortável
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.window_margin = Margin::same(12);

        // Tipografia proporcional e monoespaçada para código
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(18.0, egui::FontFamily::Proportional),
            ),
            (
                TextStyle::Body,
                FontId::new(13.5, egui::FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(12.5, egui::FontFamily::Monospace),
            ),
            (
                TextStyle::Button,
                FontId::new(13.0, egui::FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(11.0, egui::FontFamily::Proportional),
            ),
        ]
        .into();
    });
}
