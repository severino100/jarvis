//! Board views: Hoje, Agora, Revisão e QA, Refined livre, Horas do sprint (same content as the Electron version).

use crate::{
    app::View,
    azdo::{Dashboard, Item, Task},
    theme::{self, Accent},
    ui_kit::*,
};
use chrono::{Datelike, Local, Timelike};
use eframe::egui::{self, vec2, Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Ui};

const LONG_MONTHS: [&str; 12] = ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"];

pub fn task_flags(t: &Task) -> Vec<&'static str> {
    let mut f = Vec::new();
    if t.state == "Done" && t.work_done.unwrap_or(0.0) <= 0.0 {
        f.push("Done sem Work Done");
    }
    if t.estimate.unwrap_or(0.0) <= 0.0 {
        f.push("Sem estimativa");
    }
    if t.activity.as_deref().unwrap_or("").is_empty() {
        f.push("Sem Activity");
    }
    f
}

fn num(v: Option<f64>) -> String {
    // + 0.0 turns the -0.0 of an empty f64 sum into 0.
    v.map(|x| x + 0.0).map(|x| if x.fract() == 0.0 { format!("{x:.0}") } else { format!("{x:.1}") }).unwrap_or_else(|| "—".into())
}

/// List container: glass box with rows separated by hairlines.
fn list(ui: &mut Ui, acc: Accent, rows: Vec<Box<dyn FnOnce(&mut Ui) + '_>>) {
    let resp = Frame::new()
        .fill(theme::GLASS)
        .stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.14)))
        .corner_radius(CornerRadius::same(4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, row) in rows.into_iter().enumerate() {
                if i > 0 {
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                    ui.painter().rect_filled(r, 0.0, theme::alpha(acc.main, 0.14));
                }
                Frame::new().inner_margin(Margin::symmetric(16, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 4.0;
                    row(ui);
                });
            }
        })
        .response;
    brackets(ui, resp.rect, acc.main);
}

/// One item line: pills on the left, title and a sub-line, the Azure DevOps button on the right.
fn item_line(ui: &mut Ui, it: &Item, acc: Accent, sub: &str, sub_color: Color32) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(190.0, 24.0), Layout::left_to_right(Align::Center), |ui| {
            type_pill(ui, &it.kind, acc);
            id_link(ui, it.id, &it.url, acc);
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            start_button(ui, it.id, false, acc);
            ui.add_space(8.0);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.add(egui::Label::new(RichText::new(&it.title).strong().color(theme::BRIGHT)).truncate());
                if !sub.is_empty() {
                    ui.add(egui::Label::new(RichText::new(sub).small().color(sub_color)).truncate());
                }
            });
        });
    });
}

fn greeting() -> &'static str {
    match Local::now().hour() {
        0..=5 => "Boa noite",
        6..=12 => "Bom dia",
        13..=19 => "Boa tarde",
        _ => "Boa noite",
    }
}

pub fn today(ui: &mut Ui, d: &Dashboard, acc: Accent, go: &mut Option<View>) {
    let now = Local::now();
    let days_left = parse(&d.sprint.finish).map(|f| (f.date_naive() - now.date_naive()).num_days().max(0)).unwrap_or(0);
    let open_tasks = d.tasks.iter().filter(|t| t.state != "Done").count();
    let back = d.now.iter().filter(|i| i.back.is_some()).count();
    let flagged = d.tasks.iter().filter(|t| !task_flags(t).is_empty()).count();
    let first = d.me.name.split_whitespace().next().unwrap_or("");

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("{}, {first}.", greeting())).font(display(theme::size::HERO)).color(theme::BRIGHT));
            let line = format!(
                "{}, {} de {}  ◆  {}: {}  ◆  {}",
                WEEKDAYS[now.weekday().num_days_from_monday() as usize],
                now.day(),
                LONG_MONTHS[now.month0() as usize],
                d.sprint.name,
                if days_left == 0 { "último dia".to_owned() } else { format!("faltam {days_left} {}", if days_left == 1 { "dia" } else { "dias" }) },
                if open_tasks > 0 { format!("{open_tasks} tarefas abertas") } else { format!("{} tarefas, todas fechadas", d.tasks.len()) }
            )
            .replace('◆', "·");
            ui.label(RichText::new(line).monospace().color(theme::MUTED));
        });
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        {
            for (label, value, target, warn) in [
                ("Em curso", d.now.len().to_string(), View::Work, false),
                ("Devolvidos", back.to_string(), View::Work, back > 0),
                ("Em revisão", d.review.len().to_string(), View::Work, false),
                ("Horas", if flagged > 0 { flagged.to_string() } else { "ok".into() }, View::Work, flagged > 0),
            ] {
                let c = if warn { theme::AMBER } else { acc.light };
                let r = Frame::new()
                    .fill(theme::GLASS)
                    .stroke(Stroke::new(1.0, if warn { theme::alpha(theme::AMBER, 0.42) } else { theme::alpha(acc.main, 0.14) }))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::symmetric(14, 8))
                    .show(ui, |ui| {
                        ui.set_min_width(90.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(counted(ui, label, &value)).monospace().size(theme::size::STAT).strong().color(c));
                            ui.label(RichText::new(label.to_uppercase()).font(display(11.0)).color(theme::MUTED).extra_letter_spacing(1.2));
                        });
                    })
                    .response
                    .interact(Sense::click());
                if r.clicked() {
                    *go = Some(target);
                }
            }
        }
    });
    ui.add_space(14.0);

    // Day counter (the rest of the numbers arrive with PRs, agenda and tasks in phase 3).
    let closed_today = d.tasks.iter().filter(|t| t.closed_at.as_deref().is_some_and(is_today)).count();
    let est: f64 = d.tasks.iter().filter_map(|t| t.estimate).sum();
    let done: f64 = d.tasks.iter().filter_map(|t| t.work_done).sum();
    let resp = Frame::new().fill(theme::GLASS).stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.14))).corner_radius(CornerRadius::same(4)).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.columns(5, |cols| {
            cols[0].label(RichText::new("HOJE").font(display(13.0)).color(acc.main).extra_letter_spacing(3.0));
            for (col, (value, label)) in cols[1..].iter_mut().zip([
                (closed_today.to_string(), if closed_today == 1 { "tarefa fechada hoje" } else { "tarefas fechadas hoje" }),
                (open_tasks.to_string(), "tarefas abertas no sprint"),
                (format!("{} h", num(Some(done))), "Work Done no sprint"),
                (format!("{} h", num(Some(est))), "estimado no sprint"),
            ]) {
                col.label(RichText::new(counted(col, label, &value)).monospace().size(theme::size::STAT).strong().color(acc.light));
                col.label(RichText::new(label).small().color(theme::MUTED));
            }
        });
    });
    brackets(ui, resp.response.rect, acc.main);
    ui.add_space(10.0);

    ui.columns(2, |cols| {
        let left = &mut cols[0];
        let working: Vec<&Item> = d.now.iter().filter(|i| i.column == "In Progress" || i.back.is_some()).collect();
        if working.is_empty() {
            let next = d.now.first().or(d.refined.first());
            section_label(left, "Próximo passo", acc.main, acc);
            hud_panel(left, acc, false, |ui| match next {
                Some(it) => {
                    ui.horizontal_wrapped(|ui| {
                        type_pill(ui, &it.kind, acc);
                        id_link(ui, it.id, &it.url, acc);
                        column_pill(ui, &it.column, acc);
                    });
                    ui.label(RichText::new(&it.title).size(theme::size::CARD).strong().color(theme::BRIGHT));
                    ui.label(RichText::new(if d.now.is_empty() { "Não tens nada em curso: é o primeiro livre no Refined." } else { "É o próximo Planned deste sprint." }).color(theme::MUTED));
                    ui.add_space(6.0);
                    start_button(ui, it.id, true, acc);
                }
                None => {
                    ui.label(RichText::new("Nada para fazer agora.").size(theme::size::CARD).strong());
                }
            });
        } else {
            section_label(left, "Em curso", acc.main, acc);
            list(
                left,
                acc,
                working
                    .iter()
                    .map(|it| -> Box<dyn FnOnce(&mut Ui)> {
                        let (sub, c) = match &it.back {
                            Some(b) => (format!("Devolvido pelo QA ({}, {})", b.by, when(&b.at)), theme::AMBER),
                            None => ("Em In Progress".to_owned(), theme::MUTED),
                        };
                        Box::new(move |ui| item_line(ui, it, acc, &sub, c))
                    })
                    .collect(),
            );
        }
        // The item already shown as "Próximo passo" is not repeated below it.
        let next_id = if working.is_empty() { d.now.first().map(|i| i.id) } else { None };
        let later: Vec<&Item> = d.now.iter().filter(|i| i.column != "In Progress" && i.back.is_none() && Some(i.id) != next_id).collect();
        if !later.is_empty() {
            section_label(left, "A seguir", acc.main, acc);
            list(left, acc, later.iter().take(5).map(|it| -> Box<dyn FnOnce(&mut Ui)> { Box::new(move |ui| item_line(ui, it, acc, &it.column, theme::MUTED)) }).collect());
        }

        let right = &mut cols[1];
        section_label(right, "Em revisão", acc.main, acc);
        if d.review.is_empty() {
            quiet(right, "Nada em Code Review nem no QA.", acc);
        } else {
            list(
                right,
                acc,
                d.review
                    .iter()
                    .take(6)
                    .map(|it| -> Box<dyn FnOnce(&mut Ui)> {
                        let sub = format!("{} · alterado {}", it.column, when(&it.changed_at));
                        Box::new(move |ui| item_line(ui, it, acc, &sub, theme::MUTED))
                    })
                    .collect(),
            );
        }
    });
}

fn task_table(ui: &mut Ui, tasks: &[Task], acc: Accent) {
    if tasks.is_empty() {
        ui.label(RichText::new("Sem tarefas criadas.").color(theme::FAINT));
        return;
    }
    Frame::new().fill(theme::alpha(Color32::BLACK, 0.25)).stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.14))).corner_radius(CornerRadius::same(2)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new(ui.next_auto_id()).num_columns(4).spacing([16.0, 8.0]).show(ui, |ui| {
            for t in tasks {
                match &t.side {
                    Some(s) => side_pill(ui, s, acc),
                    None => {
                        ui.label("—");
                    }
                }
                ui.horizontal(|ui| {
                    id_link(ui, t.id, &t.url, acc);
                    ui.label(&t.title);
                });
                ui.label(RichText::new(t.state.to_uppercase()).font(display(12.0)).color(if t.state == "Done" { theme::GREEN } else { theme::MUTED }));
                ui.label(RichText::new(format!("est {} · feito {}", num(t.estimate), num(t.work_done))).monospace().color(theme::MUTED));
                ui.end_row();
            }
        });
    });
}

/// The full card of each item being worked on (with the QA comments of those sent back and their tasks).
fn now_panels(ui: &mut Ui, d: &Dashboard, items: &[&Item], acc: Accent) {
    let back = items.iter().filter(|i| i.back.is_some()).count();
    if back > 0 {
        hint(ui, &format!("{back} {} pelo QA. Lê os comentários antes de pegar neles.", if back > 1 { "itens foram devolvidos" } else { "item foi devolvido" }), true, acc);
        ui.add_space(8.0);
    }
    for it in items.iter().copied() {
        hud_panel(ui, acc, it.back.is_some(), |ui| {
            ui.horizontal_wrapped(|ui| {
                type_pill(ui, &it.kind, acc);
                id_link(ui, it.id, &it.url, acc);
                column_pill(ui, &it.column, acc);
                for t in it.tags.iter().filter(|t| t.eq_ignore_ascii_case("FRONT") || t.eq_ignore_ascii_case("BACK")) {
                    side_pill(ui, t, acc);
                }
                if it.owner.is_none() {
                    warn_pill(ui, "Sem dono");
                }
                if !it.in_sprint {
                    warn_pill(ui, &format!("Fora do {}", d.sprint.name));
                }
                if let Some(e) = it.effort {
                    side_pill(ui, &format!("Effort {}", num(Some(e))), acc);
                }
                if let Some(p) = it.priority {
                    side_pill(ui, &format!("P{p}"), acc);
                }
            });
            ui.label(RichText::new(&it.title).size(theme::size::CARD).strong().color(theme::BRIGHT));
            ui.label(RichText::new(format!("{}Alterado {}", it.created_by.as_ref().map(|c| format!("Criado por {c}   ")).unwrap_or_default(), when(&it.changed_at))).small().color(theme::MUTED));
            if let Some(b) = &it.back {
                ui.add_space(6.0);
                Frame::new().fill(theme::alpha(theme::AMBER, 0.1)).stroke(Stroke::new(1.0, theme::alpha(theme::AMBER, 0.42))).corner_radius(CornerRadius::same(4)).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("VOLTOU PARA TRÁS").font(display(13.0)).color(theme::AMBER).extra_letter_spacing(1.6));
                        ui.label(RichText::new(format!("{} → {}", b.from, b.to)).monospace());
                        ui.label(RichText::new(format!("{} · {}", b.by, when(&b.at))).color(theme::MUTED));
                    });
                    if b.comments.is_empty() {
                        ui.label(RichText::new("Sem comentários desde então: vê a descrição no Azure DevOps.").color(theme::MUTED));
                    }
                    for c in &b.comments {
                        ui.separator();
                        ui.label(RichText::new(format!("{} · {}", c.by, when(&c.at))).monospace().small().color(theme::MUTED));
                        if !c.text.is_empty() {
                            ui.label(&c.text);
                        }
                        if !c.images.is_empty() && button(ui, &format!("{} imagem{} no Azure DevOps", c.images.len(), if c.images.len() > 1 { "ns" } else { "" }), false, acc).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&it.url));
                        }
                    }
                });
            } else if let Some(c) = &it.last_comment {
                let text: String = c.text.chars().take(220).collect();
                ui.label(RichText::new(format!("Último comentário: {} ({}): {}", c.by, when(&c.at), if text.is_empty() { "(imagem)".into() } else { text })).color(theme::MUTED));
            }
            ui.add_space(6.0);
            task_table(ui, &it.tasks, acc);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                start_button(ui, it.id, true, acc);
                if let Some(cmd) = crate::profile::skill_command() {
                    if button(ui, "Copiar comando", false, acc).clicked() {
                        ui.ctx().copy_text(format!("{cmd} pega no {}", it.id));
                    }
                }
                if button(ui, "Abrir no Azure DevOps", false, acc).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(&it.url));
                }
            });
        });
        ui.add_space(12.0);
    }
}

fn review(ui: &mut Ui, d: &Dashboard, acc: Accent) {
    if d.review.is_empty() {
        quiet(ui, "Nada à espera de revisão ou QA. Quando tiveres itens em Code Review ou To Test aparecem aqui.", acc);
        return;
    }
    for group in ["Code Review", "Testing", "To Test"] {
        let items: Vec<&Item> = d.review.iter().filter(|i| i.column == group).collect();
        if items.is_empty() {
            continue;
        }
        section_label(ui, &format!("{group} · {}", items.len()), column_color(group, acc), acc);
        list(
            ui,
            acc,
            items
                .iter()
                .map(|it| -> Box<dyn FnOnce(&mut Ui)> {
                    let sub = match &it.last_comment {
                        Some(c) => format!("{} ({}): {}", c.by, when(&c.at), if c.text.is_empty() { "(imagem)" } else { &c.text }),
                        None => format!("Alterado {}", when(&it.changed_at)),
                    };
                    Box::new(move |ui| item_line(ui, it, acc, &sub, theme::MUTED))
                })
                .collect(),
        );
        ui.add_space(8.0);
    }
}

/// "Effort 3 · P2 · Major · por Ana": the line under a Refined item.
fn refined_sub(it: &Item) -> String {
    let mut parts = vec![it.effort.map(|e| format!("Effort {}", num(Some(e)))).unwrap_or_else(|| "Sem effort".into())];
    if let Some(p) = it.priority {
        parts.push(format!("P{p}"));
    }
    if let Some(s) = &it.severity {
        parts.push(s.clone());
    }
    if let Some(c) = &it.created_by {
        parts.push(format!("por {c}"));
    }
    parts.join(" · ")
}

/// The sections of the "Tarefas" page (the board): what the chips at the top filter.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Section {
    #[default]
    All,
    Mine,
    Doing,
    Free,
    Review,
    Hours,
}

/// "Tarefas": everything on the board in one page, in sections: Refined assigned to me, what I am doing now, what is free
/// to pick up (Planned in the sprint, or Refined outside it), Revisão e QA and Horas do sprint.
pub fn work(ui: &mut Ui, d: &Dashboard, acc: Accent, st: &mut HoursUi, board: &crate::board::Board) {
    // The column of what I am doing is the first of the board's `working` (the pack decides the names).
    let doing_col = crate::profile::board().and_then(|b| b.working.first().cloned()).unwrap_or_else(|| "In Progress".into());
    let doing: Vec<&Item> = d.now.iter().filter(|i| i.owner.is_some() && (i.column == doing_col || i.back.is_some())).collect();
    let planned_mine: Vec<&Item> = d.now.iter().filter(|i| i.owner.is_some() && i.column != doing_col && i.back.is_none()).collect();
    let free_planned: Vec<&Item> = d.now.iter().filter(|i| i.owner.is_none()).collect();
    let free_n = free_planned.len() + d.refined.len();
    let flagged = d.tasks.iter().filter(|t| !task_flags(t).is_empty()).count();

    ui.set_max_width(ui.available_width() - 14.0);
    // The numbers on top are also the filter: click one to open that section alone, click it again to see the board.
    let est: f64 = d.tasks.iter().filter_map(|t| t.estimate).sum();
    let done: f64 = d.tasks.iter().filter_map(|t| t.work_done).sum();
    let back = d.now.iter().filter(|i| i.back.is_some()).count();
    let tiles = [
        (Section::Mine, "Refined meus", d.mine_refined.len().to_string(), String::new(), false),
        (Section::Doing, "A fazer", (doing.len() + planned_mine.len()).to_string(), if back > 0 { format!("{back} devolvido{} pelo QA", if back > 1 { "s" } else { "" }) } else { format!("{} em curso", doing.len()) }, back > 0),
        (Section::Free, "Livres", free_n.to_string(), format!("{} Planned · {} Refined", free_planned.len(), d.refined.len()), false),
        (Section::Review, "Revisão e QA", d.review.len().to_string(), String::new(), false),
        (Section::Hours, "Horas", if flagged > 0 { flagged.to_string() } else { "ok".into() }, format!("{} h de {} h", num(Some(done)), num(Some(est))), flagged > 0),
    ];
    ui.columns(tiles.len(), |cols| {
        for (col, (s, label, value, sub, warn)) in cols.iter_mut().zip(tiles) {
            if stat_tile(col, acc, label, &value, &sub, warn, st.section == s).clicked() {
                st.section = if st.section == s { Section::All } else { s };
            }
        }
    });
    ui.add_space(14.0);
    if st.section == Section::All {
        let cols_n = if ui.available_width() > 1150.0 { 4 } else { 2 };
        let mine: Vec<(&Item, String, bool)> = d.mine_refined.iter().map(|it| (it, refined_sub(it), false)).collect();
        let now_cards: Vec<(&Item, String, bool)> = doing
            .iter()
            .chain(planned_mine.iter())
            .map(|it| match &it.back {
                Some(b) => (*it, format!("Devolvido pelo QA ({}, {})", b.by, when(&b.at)), true),
                None => (*it, it.column.clone(), false),
            })
            .collect();
        let free: Vec<(&Item, String, bool)> = free_planned
            .iter()
            .map(|it| (*it, format!("{} · sem dono", it.column), false))
            .chain(d.refined.iter().map(|it| (it, format!("{}{} · {}", it.column, if it.in_sprint { String::new() } else { " · fora do sprint".to_owned() }, refined_sub(it)), false)))
            .collect();
        let rev: Vec<(&Item, String, bool)> = d
            .review
            .iter()
            .map(|it| (it, format!("{} · {}", it.column, it.last_comment.as_ref().map(|c| format!("{}: {}", c.by, if c.text.is_empty() { "(imagem)" } else { &c.text })).unwrap_or_else(|| format!("alterado {}", when(&it.changed_at)))), false))
            .collect();
        let groups: [(&str, &Vec<(&Item, String, bool)>); 4] = [("Refined meus", &mine), ("A fazer agora", &now_cards), ("Livres para pegar", &free), ("Revisão e QA", &rev)];
        for row in groups.chunks(cols_n) {
            ui.columns(cols_n, |cols| {
                for (col, (title, cards)) in cols.iter_mut().zip(row) {
                    board_column(col, acc, title, cards);
                }
            });
            ui.add_space(14.0);
        }
        return;
    }
    let show = |s: Section| st.section == s;

    if show(Section::Mine) {
        section_label(ui, &format!("Atribuídas a mim em Refined · {}", d.mine_refined.len()), acc.main, acc);
        if d.mine_refined.is_empty() {
            quiet(ui, "Nenhum item refinado está atribuído a ti.", acc);
        } else {
            list(ui, acc, d.mine_refined.iter().map(|it| -> Box<dyn FnOnce(&mut Ui)> { let sub = refined_sub(it); Box::new(move |ui| item_line(ui, it, acc, &sub, theme::MUTED)) }).collect());
        }
        ui.add_space(14.0);
    }
    if show(Section::Doing) {
        section_label(ui, &format!("A fazer agora · {}", doing.len()), acc.main, acc);
        if doing.is_empty() {
            quiet(ui, "Nada em In Progress. Pega num item das listas de baixo.", acc);
        } else {
            now_panels(ui, d, &doing, acc);
        }
        if !planned_mine.is_empty() {
            section_label(ui, &format!("Planeadas para mim · {}", planned_mine.len()), acc.main, acc);
            list(ui, acc, planned_mine.iter().map(|it| -> Box<dyn FnOnce(&mut Ui)> { Box::new(move |ui| item_line(ui, it, acc, &it.column, theme::MUTED)) }).collect());
        }
        ui.add_space(14.0);
    }
    if show(Section::Free) {
        section_label(ui, &format!("Livres para pegar · {free_n}"), acc.main, acc);
        if !doing.is_empty() || !planned_mine.is_empty() {
            hint(ui, &format!("Ainda tens {} {} em curso ou planeados; o resto só entra depois.", doing.len() + planned_mine.len(), if doing.len() + planned_mine.len() > 1 { "itens" } else { "item" }), false, acc);
            ui.add_space(8.0);
        }
        if free_n == 0 {
            quiet(ui, "Nada livre: nem Planned sem dono neste sprint, nem Refined sem dono.", acc);
        } else {
            let rows: Vec<Box<dyn FnOnce(&mut Ui) + '_>> = free_planned
                .iter()
                .map(|it| -> Box<dyn FnOnce(&mut Ui)> { Box::new(move |ui| item_line(ui, it, acc, &format!("{} · {} · sem dono", it.column, d.sprint.name), theme::MUTED)) })
                .chain(d.refined.iter().map(|it| -> Box<dyn FnOnce(&mut Ui)> {
                    let sub = format!("{}{} · {}", it.column, if it.in_sprint { String::new() } else { format!(" · fora do {}", d.sprint.name) }, refined_sub(it));
                    Box::new(move |ui| item_line(ui, it, acc, &sub, theme::MUTED))
                }))
                .collect();
            list(ui, acc, rows);
        }
        ui.add_space(14.0);
    }
    if show(Section::Review) {
        section_label(ui, &format!("Revisão e QA · {}", d.review.len()), acc.main, acc);
        review(ui, d, acc);
        ui.add_space(14.0);
    }
    if show(Section::Hours) {
        section_label(ui, "Horas do sprint", acc.main, acc);
        hours(ui, d, acc, st, board);
    }
}

/// Inline form to fill a task's missing fields (Work Done, Estimate, Activity) straight in Azure DevOps.
#[derive(Default)]
pub struct HoursUi {
    /// The section chip chosen on the Tarefas page.
    pub section: Section,
    inputs: std::collections::HashMap<i64, (String, String, bool)>,
    status: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<i64, (bool, String)>>>,
}

fn fix_row(ui: &mut Ui, t: &Task, fix: &mut HoursUi, board: &crate::board::Board, acc: Accent) {
    let flags = task_flags(t);
    let needs_done = flags.contains(&"Done sem Work Done");
    let needs_est = flags.contains(&"Sem estimativa");
    let needs_act = flags.contains(&"Sem Activity");
    let (est, wd, dev) = fix.inputs.entry(t.id).or_insert_with(|| (String::new(), String::new(), true));
    hud_panel(ui, acc, true, |ui| {
        ui.horizontal_wrapped(|ui| {
            id_link(ui, t.id, &t.url, acc);
            ui.add(egui::Label::new(RichText::new(&t.title).strong()).truncate());
            warn_pill(ui, &flags.join(" · "));
        });
        ui.horizontal(|ui| {
            if needs_est {
                ui.label("Estimativa (h)");
                ui.add(egui::TextEdit::singleline(est).desired_width(56.0));
            }
            if needs_done {
                ui.label("Work Done (h)");
                ui.add(egui::TextEdit::singleline(wd).desired_width(56.0));
            }
            if needs_act {
                ui.checkbox(dev, "Activity = Development");
            }
            let status = fix.status.lock().unwrap().get(&t.id).cloned();
            if button(ui, "Guardar no Azure DevOps", true, acc).clicked() {
                let parse = |s: &str, need: bool| -> Result<Option<f64>, ()> {
                    if !need || s.trim().is_empty() {
                        return Ok(None);
                    }
                    match s.trim().replace(',', ".").parse::<f64>() {
                        Ok(v) if v > 0.0 && v <= 80.0 => Ok(Some(v)),
                        _ => Err(()),
                    }
                };
                match (parse(est, needs_est), parse(wd, needs_done)) {
                    (Ok(e), Ok(w)) => {
                        let mut fields: Vec<(&'static str, serde_json::Value)> = Vec::new();
                        if let Some(e) = e {
                            fields.push(("Custom.Estimate", e.into()));
                        }
                        if let Some(w) = w {
                            fields.push(("Custom.WorkDone", w.into()));
                        }
                        if needs_act && *dev {
                            fields.push(("Microsoft.VSTS.Common.Activity", "Development".into()));
                        }
                        if fields.is_empty() {
                            fix.status.lock().unwrap().insert(t.id, (false, "Preenche pelo menos um campo.".into()));
                        } else {
                            let (id, st, b) = (t.id, fix.status.clone(), board.clone());
                            std::thread::spawn(move || {
                                let r = crate::azdo::set_fields(id, &fields);
                                st.lock().unwrap().insert(id, match r {
                                    Ok(()) => (true, "Guardado.".into()),
                                    Err(e) => (false, format!("O Azure DevOps recusou: {}", e.lines().next().unwrap_or_default())),
                                });
                                b.refresh();
                            });
                        }
                    }
                    _ => {
                        fix.status.lock().unwrap().insert(t.id, (false, "Horas inválidas (entre 0 e 80).".into()));
                    }
                }
            }
            if let Some((ok, msg)) = status {
                ui.label(RichText::new(msg).color(if ok { theme::GREEN } else { theme::RED }));
            }
        });
    });
    ui.add_space(6.0);
}

fn hours(ui: &mut Ui, d: &Dashboard, acc: Accent, fix: &mut HoursUi, board: &crate::board::Board) {
    let tasks = &d.tasks;
    if tasks.is_empty() {
        quiet(ui, "Sem tarefas neste sprint. As tarefas que criares aparecem aqui com as horas.", acc);
        return;
    }
    let est: f64 = tasks.iter().filter_map(|t| t.estimate).sum();
    let done: f64 = tasks.iter().filter_map(|t| t.work_done).sum();
    let open = tasks.iter().filter(|t| t.state != "Done").count();
    let flagged: Vec<&Task> = tasks.iter().filter(|t| !task_flags(t).is_empty()).collect();
    ui.columns(4, |cols| {
        for (col, (label, value, small, warn)) in cols.iter_mut().zip([
            ("Estimado", num(Some(est)), " h".to_owned(), false),
            ("Work Done", num(Some(done)), " h".to_owned(), false),
            ("Tarefas abertas", open.to_string(), format!(" de {}", tasks.len()), false),
            ("Por corrigir", flagged.len().to_string(), String::new(), !flagged.is_empty()),
        ]) {
            let c = if warn { theme::AMBER } else { acc.light };
            hud_panel(col, acc, warn, |ui| {
                ui.label(RichText::new(label.to_uppercase()).font(display(11.5)).color(theme::MUTED).extra_letter_spacing(1.6));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(counted(ui, label, &value)).monospace().size(theme::size::STAT).strong().color(c));
                    ui.label(RichText::new(small).monospace().color(theme::MUTED));
                });
            });
        }
    });
    ui.add_space(10.0);
    if !flagged.is_empty() {
        section_label(ui, &format!("{} {} campos em falta", flagged.len(), if flagged.len() > 1 { "tarefas têm" } else { "tarefa tem" }), theme::AMBER, acc);
        for t in &flagged {
            fix_row(ui, t, fix, board, acc);
        }
        ui.add_space(10.0);
    }
    let mut groups: Vec<(Option<i64>, Vec<&Task>)> = Vec::new();
    for t in tasks {
        match groups.iter_mut().find(|(p, _)| *p == t.parent_id) {
            Some((_, v)) => v.push(t),
            None => groups.push((t.parent_id, vec![t])),
        }
    }
    let resp = Frame::new().fill(theme::GLASS).stroke(Stroke::new(1.0, theme::alpha(acc.main, 0.14))).corner_radius(CornerRadius::same(4)).inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // Grid cells give truncating labels no width of their own: size the title column explicitly.
        let title_w = (ui.available_width() - 520.0).max(220.0);
        egui::Grid::new("hours").num_columns(5).spacing([18.0, 9.0]).striped(false).show(ui, |ui| {
            for h in ["LADO", "TAREFA", "ESTADO", "ESTIMATIVA", "WORK DONE"] {
                ui.label(RichText::new(h).font(display(11.5)).color(theme::MUTED).extra_letter_spacing(1.6));
            }
            ui.end_row();
            for (_, ts) in &groups {
                let parent = ts[0].parent.as_ref();
                let pe: f64 = ts.iter().filter_map(|t| t.estimate).sum();
                let pd: f64 = ts.iter().filter_map(|t| t.work_done).sum();
                ui.label("");
                ui.allocate_ui_with_layout(vec2(title_w, 22.0), Layout::left_to_right(Align::Center), |ui| match parent {
                    Some(p) => {
                        ui.set_width(title_w);
                        id_link(ui, p.id, &p.url, acc);
                        ui.add(egui::Label::new(RichText::new(&p.title).strong()).truncate());
                    }
                    None => {
                        ui.label(RichText::new("Sem item pai").strong());
                    }
                });
                let col = parent.map(|p| p.column.as_str()).unwrap_or("");
                ui.label(RichText::new(col.to_uppercase()).font(display(12.0)).color(column_color(col, acc)));
                ui.label(RichText::new(num(Some(pe))).monospace().strong());
                ui.label(RichText::new(num(Some(pd))).monospace().strong());
                ui.end_row();
                for t in ts {
                    match &t.side {
                        Some(s) => side_pill(ui, s, acc),
                        None => {
                            ui.label("—");
                        }
                    }
                    ui.allocate_ui_with_layout(vec2(title_w, 22.0), Layout::left_to_right(Align::Center), |ui| {
                        ui.set_width(title_w);
                        id_link(ui, t.id, &t.url, acc);
                        ui.add(egui::Label::new(&t.title).truncate());
                        let f = task_flags(t);
                        if !f.is_empty() {
                            ui.label(RichText::new(f.join(" · ")).small().color(theme::AMBER));
                        }
                    });
                    ui.label(RichText::new(t.state.to_uppercase()).font(display(12.0)).color(if t.state == "Done" { theme::GREEN } else { theme::MUTED }));
                    ui.label(RichText::new(num(t.estimate)).monospace());
                    ui.label(RichText::new(num(t.work_done)).monospace());
                    ui.end_row();
                }
            }
        });
    });
    brackets(ui, resp.response.rect, acc.main);
    let _ = short_date;
}

/// "114 h", "18", "ok": the number counts up when it first shows (and glides when it changes); the rest stays.
fn counted(ui: &Ui, salt: &str, value: &str) -> String {
    let digits: String = value.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    let Ok(n) = digits.replace(',', ".").parse::<f64>() else { return value.to_owned() };
    let shown = crate::motion::count(ui, salt, n);
    let whole = !digits.contains(['.', ',']);
    let text = if whole { format!("{}", shown.round() as i64) } else { format!("{shown:.1}").replace('.', ",") };
    format!("{text}{}", &value[digits.len()..])
}

/// A number with its label, clickable: the tiles at the top of Tarefas.
fn stat_tile(ui: &mut Ui, acc: Accent, label: &str, value: &str, sub: &str, warn: bool, selected: bool) -> egui::Response {
    let c = if warn { theme::AMBER } else { acc.light };
    let edge = if warn { theme::alpha(theme::AMBER, 0.42) } else if selected { theme::alpha(acc.main, 0.8) } else { theme::alpha(acc.main, 0.18) };
    Frame::new()
        .fill(if selected { theme::alpha(acc.main, 0.13) } else { theme::GLASS })
        .stroke(Stroke::new(if selected { 1.5 } else { 1.0 }, edge))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(66.0);
            ui.label(RichText::new(label.to_uppercase()).font(display(11.0)).color(theme::MUTED).extra_letter_spacing(1.2));
            ui.label(RichText::new(counted(ui, label, value)).monospace().size(theme::size::STAT).strong().color(c));
            ui.add(egui::Label::new(RichText::new(sub).small().color(theme::MUTED)).truncate());
        })
        .response
        .interact(Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// One column of the board on Tarefas: a title with its count and a compact card per item.
fn board_column(ui: &mut Ui, acc: Accent, title: &str, cards: &[(&Item, String, bool)]) {
    section_label(ui, &format!("{title} · {}", cards.len()), acc.main, acc);
    if cards.is_empty() {
        quiet(ui, "Nada aqui.", acc);
        return;
    }
    for (it, sub, warn) in cards.iter().take(8) {
        Frame::new()
            .fill(theme::GLASS)
            .stroke(Stroke::new(1.0, if *warn { theme::alpha(theme::AMBER, 0.42) } else { theme::alpha(acc.main, 0.14) }))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(12, 9))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    type_pill(ui, &it.kind, acc);
                    id_link(ui, it.id, &it.url, acc);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| start_button(ui, it.id, false, acc));
                });
                ui.add(egui::Label::new(RichText::new(&it.title).strong().color(theme::BRIGHT)).wrap());
                ui.add(egui::Label::new(RichText::new(sub).small().color(if *warn { theme::AMBER } else { theme::MUTED })).truncate());
                if !it.tasks.is_empty() {
                    let done = it.tasks.iter().filter(|t| t.state == "Done").count();
                    ui.add(egui::ProgressBar::new(done as f32 / it.tasks.len() as f32).desired_height(4.0).fill(acc.main));
                }
            });
        ui.add_space(6.0);
    }
    if cards.len() > 8 {
        ui.label(RichText::new(format!("+ {} (abre a secção)", cards.len() - 8)).small().color(theme::FAINT));
    }
}
