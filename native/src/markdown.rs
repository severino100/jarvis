//! The small markdown subset Claude writes in the chat: paragraphs, headings, lists, code blocks, tables, bold and
//! inline code. Drawn with egui text layouts (no web view).

use crate::theme::{self, Accent};
use eframe::egui::{self, text::LayoutJob, Color32, CornerRadius, FontFamily, FontId, Frame, Margin, RichText, Stroke, TextFormat, Ui};

const BODY: f32 = 14.5;

/// Inline spans: **bold**, `code`, [text](url) (shown as text), *em* (shown plain).
fn inline(job: &mut LayoutJob, text: &str, size: f32, color: Color32, bold_all: bool, acc: Accent) {
    let base = |bold: bool| TextFormat {
        font_id: FontId::new(size, FontFamily::Proportional),
        color: if bold { theme::BRIGHT } else { color },
        ..Default::default()
    };
    let code = TextFormat {
        font_id: FontId::monospace(size - 1.5),
        color: acc.light,
        background: theme::alpha(acc.main, 0.12),
        ..Default::default()
    };
    let mut bold = bold_all;
    let mut rest = text;
    let mut plain = String::new();
    let flush = |job: &mut LayoutJob, plain: &mut String, bold: bool| {
        if !plain.is_empty() {
            job.append(plain, 0.0, base(bold));
            plain.clear();
        }
    };
    while let Some(ch) = rest.chars().next() {
        if rest.starts_with("**") {
            flush(job, &mut plain, bold);
            bold = !bold || bold_all;
            rest = &rest[2..];
        } else if ch == '`' {
            if let Some(end) = rest[1..].find('`') {
                flush(job, &mut plain, bold);
                job.append(&rest[1..1 + end], 0.0, code.clone());
                rest = &rest[end + 2..];
            } else {
                plain.push(ch);
                rest = &rest[1..];
            }
        } else if ch == '[' {
            let close = rest.find("](");
            let end = close.and_then(|c| rest[c..].find(')').map(|e| c + e));
            match (close, end) {
                (Some(c), Some(e)) if !rest[1..c].contains('\n') => {
                    plain.push_str(&rest[1..c]);
                    rest = &rest[e + 1..];
                }
                _ => {
                    plain.push(ch);
                    rest = &rest[1..];
                }
            }
        } else {
            plain.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    flush(job, &mut plain, bold);
}

fn text_line(ui: &mut Ui, text: &str, size: f32, color: Color32, bold: bool, acc: Accent) {
    let mut job = LayoutJob::default();
    inline(&mut job, text, size, color, bold, acc);
    job.wrap.max_width = ui.available_width();
    ui.label(job);
}

fn is_table_row(l: &str) -> bool {
    let t = l.trim();
    t.starts_with('|') && t.len() > 1
}

fn cells(l: &str) -> Vec<String> {
    let t = l.trim().trim_start_matches('|').trim_end_matches('|');
    t.split('|').map(|c| c.trim().to_owned()).collect()
}

fn list_marker(l: &str) -> Option<(usize, String, &str)> {
    let indent = l.len() - l.trim_start().len();
    let t = l.trim_start();
    if let Some(r) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("• ")) {
        return Some((indent, "•".into(), r));
    }
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        if let Some(r) = t[digits.len()..].strip_prefix(". ").or_else(|| t[digits.len()..].strip_prefix(") ")) {
            return Some((indent, format!("{digits}."), r));
        }
    }
    None
}

pub fn show(ui: &mut Ui, text: &str, acc: Accent) {
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    let mut para: Vec<&str> = Vec::new();
    let flush_para = |ui: &mut Ui, para: &mut Vec<&str>| {
        if !para.is_empty() {
            text_line(ui, &para.join(" "), BODY, theme::TEXT, false, acc);
            ui.add_space(6.0);
            para.clear();
        }
    };
    ui.spacing_mut().item_spacing.y = 3.0;
    while i < lines.len() {
        let l = lines[i];
        let t = l.trim();
        if t.starts_with("```") {
            flush_para(ui, &mut para);
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            Frame::new()
                .fill(theme::alpha(acc.main, 0.07))
                .stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.16)))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(RichText::new(code.join("\n")).monospace().size(12.5).color(theme::TEXT)).wrap());
                });
            ui.add_space(6.0);
        } else if t.is_empty() {
            flush_para(ui, &mut para);
        } else if let Some(h) = t.strip_prefix('#') {
            flush_para(ui, &mut para);
            let level = 1 + h.chars().take_while(|c| *c == '#').count();
            let h = h.trim_start_matches('#').trim();
            ui.add_space(4.0);
            let size = if level <= 2 { 18.0 } else { 16.0 };
            ui.label(RichText::new(h.replace("**", "").replace('`', "")).font(crate::ui_kit::display(size)).color(if level <= 2 { theme::BRIGHT } else { acc.light }));
            ui.add_space(2.0);
        } else if is_table_row(l) {
            flush_para(ui, &mut para);
            let mut rows: Vec<Vec<String>> = Vec::new();
            while i < lines.len() && is_table_row(lines[i]) {
                let r = cells(lines[i]);
                let separator = r.iter().all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')));
                if !separator {
                    rows.push(r);
                }
                i += 1;
            }
            table(ui, &rows, acc);
            ui.add_space(6.0);
            continue;
        } else if let Some((indent, marker, rest)) = list_marker(l) {
            flush_para(ui, &mut para);
            ui.horizontal_top(|ui| {
                ui.add_space(6.0 + (indent as f32 / 2.0).min(3.0) * 14.0);
                ui.label(RichText::new(marker).size(BODY).color(acc.main));
                ui.vertical(|ui| text_line(ui, rest, BODY, theme::TEXT, false, acc));
            });
        } else if let Some(q) = t.strip_prefix('>') {
            flush_para(ui, &mut para);
            ui.horizontal_top(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(3.0, 18.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, theme::alpha(acc.main, 0.5));
                ui.vertical(|ui| text_line(ui, q.trim(), BODY, theme::MUTED, false, acc));
            });
        } else if t.chars().all(|c| c == '-' || c == '*' || c == '_') && t.len() >= 3 {
            flush_para(ui, &mut para);
            ui.separator();
        } else {
            para.push(t);
        }
        i += 1;
    }
    flush_para(ui, &mut para);
}

fn table(ui: &mut Ui, rows: &[Vec<String>], acc: Accent) {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return;
    }
    let col_w = ((ui.available_width() - 24.0) / cols as f32).clamp(60.0, 360.0);
    Frame::new().stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.16))).corner_radius(CornerRadius::same(4)).inner_margin(Margin::same(8)).show(ui, |ui| {
        egui::Grid::new(ui.next_auto_id()).striped(true).spacing([14.0, 6.0]).min_col_width(40.0).max_col_width(col_w).show(ui, |ui| {
            for (r, row) in rows.iter().enumerate() {
                for c in 0..cols {
                    let cell = row.get(c).map(String::as_str).unwrap_or("");
                    let mut job = LayoutJob::default();
                    inline(&mut job, cell, 13.5, if r == 0 { acc.light } else { theme::TEXT }, r == 0, acc);
                    job.wrap.max_width = col_w;
                    ui.label(job);
                }
                ui.end_row();
            }
        });
    });
}
