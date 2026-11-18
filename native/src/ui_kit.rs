//! Shared HUD pieces: pills, panels with corner brackets, section labels, relative dates.

use crate::theme::{self, Accent};
use chrono::{DateTime, Datelike, Local, Timelike};
use eframe::egui::{self, pos2, vec2, Color32, CornerRadius, FontId, Frame, Margin, Response, RichText, Sense, Stroke, Ui};

pub const MONTHS: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];
pub const WEEKDAYS: [&str; 7] = ["segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado", "domingo"];

pub fn parse(iso: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(iso).ok().map(|d| d.with_timezone(&Local))
}

pub fn is_today(iso: &str) -> bool {
    parse(iso).is_some_and(|d| d.date_naive() == Local::now().date_naive())
}

/// "agora mesmo", "há 12 min", "hoje 09:40", "ontem 17:02", "17 nov".
pub fn when(iso: &str) -> String {
    let Some(d) = parse(iso) else { return String::new() };
    let now = Local::now();
    let mins = (now - d).num_minutes();
    if mins < 1 {
        return "agora mesmo".into();
    }
    if mins < 60 {
        return format!("há {mins} min");
    }
    let days = (now.date_naive() - d.date_naive()).num_days();
    let hm = format!("{:02}:{:02}", d.hour(), d.minute());
    match days {
        0 => format!("hoje {hm}"),
        1 => format!("ontem {hm}"),
        _ => format!("{} {}", d.day(), MONTHS[d.month0() as usize]),
    }
}

pub fn short_date(iso: &str) -> String {
    parse(iso).map(|d| format!("{:02}/{:02}", d.day(), d.month())).unwrap_or_default()
}

pub fn display(size: f32) -> FontId {
    FontId::new(size, theme::display())
}

/// Small uppercase tag (type, column, side, warnings).
pub fn pill(ui: &mut Ui, text: &str, fg: Color32, fill: Color32, border: Color32) -> Response {
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(2))
        .inner_margin(Margin::symmetric(7, 2))
        .show(ui, |ui| ui.label(RichText::new(text.to_uppercase()).font(display(11.5)).color(fg).extra_letter_spacing(0.8)))
        .response
}

pub fn type_pill(ui: &mut Ui, kind: &str, acc: Accent) {
    let (label, c) = match kind {
        "Bug" => ("Bug", theme::RED),
        "Improvement" => ("Improvement", Color32::from_rgb(0x4f, 0xf0, 0xe0)),
        "Product Backlog Item" => ("PBI", acc.main),
        "Change Request" => ("CR", Color32::from_rgb(0xb4, 0x9c, 0xff)),
        other => (other, theme::MUTED),
    };
    pill(ui, label, c, theme::alpha(c, 0.1), theme::alpha(c, 0.35));
}

pub fn column_color(column: &str, acc: Accent) -> Color32 {
    match column {
        "In Progress" => acc.main,
        "Planned" => Color32::from_rgb(0xb4, 0x9c, 0xff),
        "Code Review" => Color32::from_rgb(0x4f, 0xf0, 0xe0),
        "To Test" | "Testing" => theme::GREEN,
        _ => theme::MUTED,
    }
}

/// Column as a glowing diamond plus the name, no box.
pub fn column_pill(ui: &mut Ui, column: &str, acc: Accent) {
    let c = column_color(column, acc);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        diamond(ui, true, c, 3.5);
        ui.label(RichText::new(column.to_uppercase()).font(display(11.5)).color(c).extra_letter_spacing(0.8));
    });
}

pub fn side_pill(ui: &mut Ui, side: &str, acc: Accent) {
    pill(ui, side, theme::TEXT, theme::alpha(acc.main, 0.04), theme::alpha(acc.main, 0.14));
}

pub fn warn_pill(ui: &mut Ui, text: &str) {
    pill(ui, text, theme::AMBER, theme::alpha(theme::AMBER, 0.1), theme::alpha(theme::AMBER, 0.42));
}

/// "#8125" in the accent colour; clicking opens the item in Azure DevOps.
pub fn id_link(ui: &mut Ui, id: i64, url: &str, acc: Accent) {
    let r = ui.add(egui::Label::new(RichText::new(format!("#{id}")).monospace().strong().color(acc.main)).sense(Sense::click()));
    if r.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
    r.on_hover_text("Abrir no Azure DevOps");
}

pub fn diamond(ui: &mut Ui, filled: bool, color: Color32, d: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(d * 2.0 + 2.0, d * 2.0 + 2.0), Sense::hover());
    let c = rect.center();
    let pts = vec![pos2(c.x, c.y - d), pos2(c.x + d, c.y), pos2(c.x, c.y + d), pos2(c.x - d, c.y)];
    let shape = if filled { egui::Shape::convex_polygon(pts, color, Stroke::NONE) } else { egui::Shape::closed_line(pts, Stroke::new(1.2, color)) };
    ui.painter().add(shape);
}

/// Glass panel with the accent corner brackets; `warn` paints them amber.
pub fn hud_panel(ui: &mut Ui, acc: Accent, warn: bool, add: impl FnOnce(&mut Ui)) -> Response {
    let edge = if warn { theme::alpha(theme::AMBER, 0.42) } else { theme::alpha(acc.main, 0.14) };
    let resp = Frame::new()
        .fill(theme::GLASS)
        .stroke(Stroke::new(1.0, edge))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(20, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Panels inside columns inherit a justified layout, which stretches text: keep it left-aligned.
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), add);
        })
        .response;
    let color = if warn { theme::AMBER } else { acc.main };
    // Glass edge: a faint light along the top, as if lit from above.
    let r = resp.rect;
    ui.painter().line_segment([pos2(r.left() + 18.0, r.top() + 0.5), pos2(r.right() - 18.0, r.top() + 0.5)], Stroke::new(1.0, theme::alpha(theme::BRIGHT, 0.07)));
    let h = ui.ctx().animate_bool_with_time_and_easing(resp.id.with("panel-hover"), ui.rect_contains_pointer(r), 0.22, egui::emath::easing::cubic_out);
    brackets_len(ui, r, if h > 0.0 { theme::alpha(color, 0.75 + 0.25 * h) } else { color }, 14.0 + 12.0 * h);
    resp
}

pub fn brackets(ui: &Ui, r: egui::Rect, color: Color32) {
    brackets_len(ui, r, color, 14.0);
}

fn brackets_len(ui: &Ui, r: egui::Rect, color: Color32, k: f32) {
    let p = ui.painter();
    let s = Stroke::new(1.0, color);
    p.line_segment([r.left_top(), r.left_top() + vec2(k, 0.0)], s);
    p.line_segment([r.left_top(), r.left_top() + vec2(0.0, k)], s);
    p.line_segment([r.right_bottom(), r.right_bottom() - vec2(k, 0.0)], s);
    p.line_segment([r.right_bottom(), r.right_bottom() - vec2(0.0, k)], s);
}

/// "EM CURSO ———— link" header above a block.
pub fn section_label(ui: &mut Ui, text: &str, color: Color32, acc: Accent) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(text.to_uppercase()).font(display(12.5)).color(color).extra_letter_spacing(2.2));
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 12.0), Sense::hover());
        ui.painter().line_segment([pos2(rect.left() + 6.0, rect.center().y), pos2(rect.right(), rect.center().y)], Stroke::new(1.0, theme::alpha(acc.main, 0.3)));
    });
    ui.add_space(2.0);
}

/// Hint bar (info in accent, warning in amber).
pub fn hint(ui: &mut Ui, text: &str, warn: bool, acc: Accent) {
    let c = if warn { theme::AMBER } else { acc.main };
    Frame::new()
        .fill(theme::alpha(c, 0.1))
        .stroke(Stroke::new(1.0, theme::alpha(c, 0.42)))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                diamond(ui, true, c, 4.0);
                ui.label(RichText::new(text).color(theme::TEXT));
            });
        });
}

/// Primary (filled accent) or secondary (outline) uppercase button.
pub fn button(ui: &mut Ui, text: &str, primary: bool, acc: Accent) -> Response {
    let label = RichText::new(text.to_uppercase()).font(display(theme::size::BUTTON)).extra_letter_spacing(0.9).color(if primary { Color32::from_rgb(0x02, 0x10, 0x19) } else { theme::TEXT });
    let b = egui::Button::new(label).corner_radius(CornerRadius::same(4));
    let b = if primary { b.fill(acc.main).stroke(Stroke::new(1.0, acc.main)) } else { b.fill(theme::alpha(acc.main, 0.03)).stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.3))) };
    // The halo goes behind the button: reserve its slot before the button is painted.
    let behind = ui.painter().add(egui::Shape::Noop);
    // Inside columns or full-width panels the layout stretches widgets: a button keeps its own size there.
    let r = if ui.layout().horizontal_justify() { ui.horizontal(|ui| ui.add(b)).inner } else { ui.add(b) };
    let h = crate::motion::hover(ui, &r);
    if h > 0.01 {
        let shadow = egui::epaint::Shadow { offset: [0, 3], blur: 16, spread: 0, color: theme::alpha(acc.main, if primary { 0.45 } else { 0.3 } * h) };
        ui.painter().set(behind, shadow.as_shape(r.rect, CornerRadius::same(4)));
        ui.painter().rect_stroke(r.rect.expand(1.0 + 1.5 * h), CornerRadius::same(5), Stroke::new(1.0, theme::alpha(acc.light, 0.4 * h)), egui::StrokeKind::Outside);
    }
    r
}

pub fn quiet(ui: &mut Ui, text: &str, acc: Accent) {
    Frame::new()
        .stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.14)))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).color(theme::MUTED));
        });
}

/// "Começar" on an item anywhere in the views: the app picks it up after the frame (see `take_start`).
pub fn start_button(ui: &mut Ui, id: i64, primary: bool, acc: Accent) {
    if button(ui, "Começar", primary, acc).on_hover_text("Abre a conversa do item, atribui-to e põe-no no sprint").clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("jarvis-start"), id));
    }
}

pub fn take_start(ctx: &egui::Context) -> Option<i64> {
    ctx.data_mut(|d| d.remove_temp::<i64>(egui::Id::unique("jarvis-start")))
}

/// Same as `start_button`, with the wording that fits the place ("Continuar", "Pegar"…).
pub fn start_button_as(ui: &mut Ui, id: i64, label: &str, hint: &str, primary: bool, acc: Accent) {
    if button(ui, label, primary, acc).on_hover_text(hint).clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("jarvis-start"), id));
    }
}
