use crate::{
    analysis::pretty_identity,
    ai::{model_presets, AiConfig},
    deep_agent::OptimizationGoal,
    app::{
        App, CardZone, DeepLearnPage, Modal, Page, Screen,
        SettingsTab, SuggestionKind, TextPane, UiAction,
    },
    theme::{self, ThemeChannel, ThemeSlot},
};
use ratatui::{
    layout::{
        Alignment, Constraint, Direction, Layout, Rect,
    },
    style::{Color, Modifier, Style},
    symbols,
    text::Line,
    widgets::{
        Axis, Bar, BarChart, BarGroup, Block, Borders, Chart, Clear, Dataset,
        Gauge, GraphType, LineGauge, List, ListItem, ListState, Paragraph,
        Sparkline, Wrap,
    },
    Frame,
};
use ratatui_image::Image as TerminalImage;

pub fn render(frame: &mut Frame, app: &mut App) {
    app.regions = Default::default();

    frame.render_widget(
        Block::default().style(theme::base()),
        frame.area(),
    );

    match app.screen {
        Screen::DeckPicker => render_picker(frame, app),
        Screen::Dashboard => render_dashboard(frame, app),
        Screen::DeepLearn => render_deep_learn_screen(frame, app),
        Screen::Settings => render_settings_screen(frame, app),
    }

    if app.modal.active() {
        render_modal(frame, app);
    }

    render_runtime_hud(frame, app);
}


fn render_runtime_hud(
    frame: &mut Frame,
    app: &App,
) {
    let full_area = frame.area();

    if full_area.height == 0 || full_area.width < 24 {
        return;
    }

    let version = env!("CARGO_PKG_VERSION");
    let memory = app.runtime_stats.memory_mb();
    let cpu = app.runtime_stats.cpu_usage();
    let uptime = app.runtime_stats.uptime_compact();
    let gpu = app
        .runtime_stats
        .gpu_usage()
        .map(|value| format!("{value:.0}%"))
        .unwrap_or_else(|| "-".to_string());
    let vram = app
        .runtime_stats
        .gpu_memory_mb()
        .map(|(used, total)| format!("{used}/{total}MB"))
        .unwrap_or_else(|| "-".to_string());
    let worker = app.runtime_worker_label();

    let loaded_cards = app
        .parsed
        .as_ref()
        .map(|deck| {
            deck.commanders
                .iter()
                .chain(deck.main.iter())
                .chain(deck.sideboard.iter())
                .map(|entry| entry.quantity as usize)
                .sum::<usize>()
        })
        .unwrap_or(0);

    let text = if full_area.width >= 150 {
        format!(
            "v{} | RAM {:.0} MB | CPU {:.1}% | GPU {} | VRAM {} | UP {} | DECKS {} | CARDS {} | SOURCES {} | {}",
            version,
            memory,
            cpu,
            gpu,
            vram,
            uptime,
            app.decks.len(),
            loaded_cards,
            app.deck_sources.len(),
            worker,
        )
    } else if full_area.width >= 105 {
        format!(
            "v{} | RAM {:.0} MB | CPU {:.1}% | GPU {} | UP {} | DECKS {} | {}",
            version,
            memory,
            cpu,
            gpu,
            uptime,
            app.decks.len(),
            worker,
        )
    } else {
        format!(
            "v{} | RAM {:.0} MB | CPU {:.1}% | GPU {} | {}",
            version,
            memory,
            cpu,
            gpu,
            worker,
        )
    };

    let width = (text.chars().count() as u16)
        .saturating_add(1)
        .min(full_area.width);

    let area = Rect::new(
        full_area
            .x
            .saturating_add(full_area.width.saturating_sub(width)),
        full_area
            .y
            .saturating_add(full_area.height.saturating_sub(1)),
        width,
        1,
    );

    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Right)
            .style(
                Style::default()
                    .fg(theme::muted_color())
                    .bg(theme::bg()),
            ),
        area,
    );
}

fn render_picker(frame: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(4),
            Constraint::Length(2),
            Constraint::Min(12),
            Constraint::Length(8),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_app_header(
        frame,
        root[0],
        "COMMANDER LAB",
        "Commander deck research / generator",
        app,
    );

    if frame.area().width >= 120 {
        render_picker_summary_cards(frame, root[1], app);
        render_picker_filter_strip(frame, root[2], app);
        render_picker_main(frame, root[3], app);
        render_picker_bottom_panels(frame, root[4], app);
        render_picker_key_strip(frame, root[5], app);
    } else {
        render_picker_compact(frame, Rect::new(
            root[0].x,
            root[1].y,
            frame.area().width,
            frame.area().height.saturating_sub(4),
        ), app);
        render_picker_key_strip(frame, root[5], app);
    }
}

fn render_picker_summary_cards(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    let card_count = app.db.card_count().unwrap_or(0);

    render_summary_action_card(
        frame,
        cards[0],
        "1: DECK LIBRARY",
        app.decks.len().to_string(),
        "click selected deck below to analyze",
        theme::cyan(),
        true,
        UiAction::AnalyzeSelected,
        app,
    );

    render_summary_action_card(
        frame,
        cards[1],
        "2: CARDS / STATUS",
        format!(
            "{} • {}",
            card_count,
            if app.db.has_cards() { "READY" } else { "UPDATE" },
        ),
        "Scryfall database • click to update",
        if app.db.has_cards() {
            theme::green()
        } else {
            theme::yellow()
        },
        false,
        UiAction::Update,
        app,
    );

    render_summary_action_card(
        frame,
        cards[2],
        "3: AI AGENT",
        app.deep_learn_intensity.label().to_string(),
        "analyze selected deck with local AI",
        theme::magenta(),
        false,
        UiAction::PickerDeepLearn,
        app,
    );
}

fn render_picker_filter_strip(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    frame.render_widget(
        Paragraph::new(format!(
            "Search: deck library     [Enter: Analyze]  [N: New / Import / Generate]  [R: Refresh]  [T: Settings]     sort: [name]     {} deck(s)",
            app.decks.len()
        ))
        .style(
            Style::default()
                .fg(theme::muted_color())
                .bg(theme::bg()),
        ),
        area,
    );
}

fn render_picker_main(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(62),
            Constraint::Percentage(38),
        ])
        .split(area);

    render_deck_library_table(frame, cols[0], app);
    render_picker_detail_panel(frame, cols[1], app);
}

fn render_deck_library_table(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    app.regions.deck_list = Some(area);

    let mut lines = Vec::<Line>::new();

    lines.push(
        Line::from(format!(
            "{:<3} {:<38} {:<16} {:>8}",
            "",
            "DECK",
            "SOURCE",
            "STATUS"
        ))
        .style(
            Style::default()
                .fg(theme::cyan())
                .add_modifier(Modifier::BOLD),
        ),
    );

    for (index, deck) in app.decks.iter().enumerate() {
        let selected = index == app.selected_deck;

        lines.push(
            Line::from(format!(
                "{:<3} {:<38} {:<16} {:>8}",
                if selected { "›" } else { " " },
                truncate_text(&deck.name, 37),
                "local",
                "ready",
            ))
            .style(if selected {
                theme::selected()
            } else {
                Style::default().fg(theme::text())
            }),
        );

        // +2 accounts for the border and table header row.
        let row_y = area.y
            .saturating_add(2)
            .saturating_add(index as u16);

        if row_y < area.y.saturating_add(area.height.saturating_sub(1)) {
            app.regions.deck_rows.push((
                index,
                Rect::new(
                    area.x + 1,
                    row_y,
                    area.width.saturating_sub(2),
                    1,
                ),
            ));
        }
    }

    if app.decks.is_empty() {
        lines.push(Line::from("  No deck files found in ./decks"));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" DECK LIBRARY ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::cyan()))
                    .style(theme::panel()),
            ),
        area,
    );
}

fn render_picker_detail_panel(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let selected_name = app
        .decks
        .get(app.selected_deck)
        .map(|deck| deck.name.clone())
        .unwrap_or_else(|| "No deck selected".to_string());

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    let text = vec![
        Line::from(selected_name)
            .style(
                Style::default()
                    .fg(theme::cyan())
                    .add_modifier(Modifier::BOLD),
            ),
        Line::from(""),
        Line::from("RESEARCH PIPELINE")
            .style(
                Style::default()
                    .fg(theme::yellow())
                    .add_modifier(Modifier::BOLD),
            ),
        Line::from("  EDHREC commander shell"),
        Line::from("  Moxfield / Archidekt references"),
        Line::from("  Commander Spellbook combos"),
        Line::from("  Scryfall legality / price / art"),
        Line::from(""),
        Line::from("Generated decks preserve exact source URLs."),
    ];

    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" SELECTED DECK ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            ),
        rows[0],
    );

    render_button(
        frame,
        rows[1],
        " ANALYZE SELECTED DECK ",
        UiAction::AnalyzeSelected,
        theme::cyan(),
        app,
    );

    render_button(
        frame,
        rows[2],
        " RUN AI AGENT ",
        UiAction::PickerDeepLearn,
        theme::magenta(),
        app,
    );
}

fn render_picker_bottom_panels(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(62),
            Constraint::Percentage(38),
        ])
        .split(area);

    let activity = vec![
        Line::from("● System Ready")
            .style(Style::default().fg(theme::text())),
        Line::from(format!("  {}", app.status))
            .style(Style::default().fg(theme::muted_color())),
        Line::from(format!(
            "  database: {} cards • decks: {}",
            app.db.card_count().unwrap_or(0),
            app.decks.len(),
        ))
        .style(Style::default().fg(theme::muted_color())),
    ];

    frame.render_widget(
        Paragraph::new(activity)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" PROGRESS & ACTIVITY ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            ),
        cols[0],
    );

    let action_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(cols[1]);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(action_rows[0]);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(action_rows[1]);

    render_button(
        frame,
        top[0],
        " NEW / IMPORT ",
        UiAction::NewDeck,
        theme::cyan(),
        app,
    );
    render_button(
        frame,
        top[1],
        " RESCAN DECKS ",
        UiAction::RefreshDecks,
        theme::blue(),
        app,
    );
    render_button(
        frame,
        bottom[0],
        " UPDATE DATA ",
        UiAction::Update,
        theme::yellow(),
        app,
    );
    render_button(
        frame,
        bottom[1],
        " SETTINGS ",
        UiAction::SettingsOpen,
        theme::magenta(),
        app,
    );
}

fn render_picker_key_strip(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    frame.render_widget(
        Paragraph::new(format!(
            "{} selected     ↑/↓ or j/k navigate     Enter analyze     N new     T settings     Q quit",
            if app.decks.is_empty() { 0 } else { 1 }
        ))
        .style(
            Style::default()
                .fg(theme::cyan())
                .bg(theme::bg()),
        ),
        area,
    );
}

fn render_app_header(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    subtitle: &str,
    app: &App,
) {
    let time_text = chrono::Local::now().format("%H:%M:%S").to_string();

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),
            Constraint::Min(1),
            Constraint::Length(12),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(title)
            .style(
                Style::default()
                    .fg(theme::cyan())
                    .add_modifier(Modifier::BOLD),
            ),
        cols[0],
    );

    frame.render_widget(
        Paragraph::new(format!("{}     {}", subtitle, app.status))
            .style(Style::default().fg(theme::muted_color())),
        cols[1],
    );

    frame.render_widget(
        Paragraph::new(time_text)
            .alignment(Alignment::Right)
            .style(Style::default().fg(theme::blue())),
        cols[2],
    );
}

fn render_summary_card(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    value: String,
    subtitle: &str,
    accent: Color,
    selected: bool,
) {
    let border = if selected { theme::cyan() } else { accent };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!(" {} ", value))
                .style(
                    Style::default()
                        .fg(accent)
                        .add_modifier(Modifier::BOLD),
                ),
            Line::from(format!(" {}", subtitle))
                .style(Style::default().fg(theme::muted_color())),
        ])
        .block(
            Block::default()
                .title(format!(" {title} "))
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(border))
                .style(theme::panel()),
        ),
        area,
    );
}

fn render_summary_action_card(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    value: String,
    subtitle: &str,
    accent: Color,
    selected: bool,
    action: UiAction,
    app: &mut App,
) {
    app.regions.buttons.push((action, area));

    render_summary_card(
        frame,
        area,
        title,
        value,
        subtitle,
        accent,
        selected,
    );
}


fn truncate_text(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();

    if count <= max_chars {
        return value.to_string();
    }

    let mut text = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();

    text.push('…');
    text
}

fn render_picker_compact(frame: &mut Frame, area: Rect, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(10),
        ])
        .split(area);

    render_banner(
        frame,
        outer[0],
        "COMMANDER LAB",
        "LOCAL COMMANDER DECK WORKBENCH",
    );

    render_deck_library_panel(frame, outer[1], app);
}

fn render_deck_library_panel(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    app.regions.deck_list = Some(area);

    let items = if app.decks.is_empty() {
        vec![ListItem::new("  No deck files found in ./decks")]
    } else {
        app.decks
            .iter()
            .map(|deck| ListItem::new(format!("  {}", deck.name)))
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title(" DECK LIBRARY ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::orange()))
                .style(theme::panel()),
        )
        .highlight_symbol("› ")
        .highlight_style(theme::selected());

    let mut state = ListState::default();

    if !app.decks.is_empty() {
        state.select(Some(app.selected_deck));
    }

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_dashboard(frame: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(8),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_app_header(
        frame,
        root[0],
        "COMMANDER LAB",
        &dashboard_subtitle(app),
        app,
    );

    if frame.area().width >= 135 {
        render_dashboard_summary_cards(frame, root[1], app);
        render_clickable_tabs(frame, root[2], app);
        render_dashboard_wide(frame, root[3], app);
        render_dashboard_bottom_panels(frame, root[4], app);
        render_dashboard_key_strip(frame, root[5], app);
    } else {
        render_dashboard_compact(
            frame,
            Rect::new(
                root[0].x,
                root[1].y,
                frame.area().width,
                frame.area().height.saturating_sub(4),
            ),
            app,
        );
        render_dashboard_key_strip(frame, root[5], app);
    }
}

fn render_dashboard_summary_cards(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(area);

    let Some(analysis) = app.analysis.as_ref() else {
        return;
    };

    // Copy everything needed from the immutable analysis borrow before
    // passing &mut App into any clickable card renderer.
    let total_cards = analysis.total_cards.to_string();
    let bracket_number = analysis.bracket.bracket.to_string();
    let bracket_name = analysis.bracket.display_name();
    let (deck_price, _) = analysis_price_values(analysis);
    let price = format_deck_price(deck_price);

    let combo_count = app
        .combos
        .as_ref()
        .map(|report| report.included.to_string())
        .unwrap_or_else(|| "-".to_string());

    let deep_learn_value = app
        .deep_learn
        .as_ref()
        .map(|report| format!("{}%", report.confidence))
        .unwrap_or_else(|| "READY".to_string());

    let deep_learn_label =
        app.deep_learn_intensity.label().to_string();

    let cards_selected = app.page == Page::Cards;
    let bracket_selected = app.page == Page::Bracket;
    let combos_selected = app.page == Page::Combos;

    render_summary_action_card(
        frame,
        cards[0],
        "CARDS",
        total_cards,
        "commander list",
        theme::cyan(),
        cards_selected,
        UiAction::OpenCardsPage,
        app,
    );

    render_summary_action_card(
        frame,
        cards[1],
        "BRACKET",
        bracket_number,
        bracket_name.as_str(),
        theme::magenta(),
        bracket_selected,
        UiAction::OpenBracketPage,
        app,
    );

    render_summary_card(
        frame,
        cards[2],
        "PRICE",
        price,
        "current Scryfall",
        theme::yellow(),
        false,
    );

    render_summary_action_card(
        frame,
        cards[3],
        "COMBOS",
        combo_count,
        "Commander Spellbook",
        theme::purple(),
        combos_selected,
        UiAction::OpenCombosPage,
        app,
    );

    render_summary_action_card(
        frame,
        cards[4],
        "AI AGENT",
        deep_learn_value,
        deep_learn_label.as_str(),
        theme::orange(),
        false,
        UiAction::OpenDeepLearnPage,
        app,
    );
}

fn render_dashboard_wide(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(64),
            Constraint::Percentage(36),
        ])
        .split(area);

    match app.page {
        Page::Overview => render_overview(frame, cols[0], app),
        Page::Cards => render_cards(frame, cols[0], app),
        Page::Mana => render_mana(frame, cols[0], app),
        Page::Curve => render_curve(frame, cols[0], app),
        Page::Roles => render_roles(frame, cols[0], app),
        Page::Legality => render_legality(frame, cols[0], app),
        Page::Suggestions => render_suggestions(frame, cols[0], app),
        Page::Bracket => render_bracket(frame, cols[0], app),
        Page::Simulation => render_simulation(frame, cols[0], app),
        Page::Combos => render_combos(frame, cols[0], app),
        Page::Sources => render_sources(frame, cols[0], app),
        Page::Mpc => render_mpc(frame, cols[0], app),
        Page::Log => render_deck_log(frame, cols[0], app),
        Page::Help => render_help(frame, cols[0]),
    }

    render_dashboard_detail_column(frame, cols[1], app);
}

fn render_dashboard_detail_column(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(58),
            Constraint::Percentage(42),
        ])
        .split(area);

    let mut detail = Vec::<Line>::new();

    if let Some(analysis) = app.analysis.as_ref() {
        detail.push(
            Line::from(analysis.deck_name.as_str())
                .style(
                    Style::default()
                        .fg(theme::cyan())
                        .add_modifier(Modifier::BOLD),
                ),
        );
        detail.push(Line::from(""));
        detail.push(Line::from(format!(
            "Commander: {}",
            analysis.commanders.join(" + ")
        )));
        detail.push(Line::from(format!(
            "Identity: {}",
            pretty_identity(&analysis.identity)
        )));
        detail.push(Line::from(format!(
            "Bracket: {}",
            analysis.bracket.display_name()
        )));
        let (deck_price, _) = analysis_price_values(analysis);
        detail.push(Line::from(format!(
            "Price: {}",
            format_deck_price(deck_price),
        )));
        detail.push(Line::from(""));
        detail.push(
            Line::from("NORMAL BUILD SIGNALS")
                .style(
                    Style::default()
                        .fg(theme::yellow())
                        .add_modifier(Modifier::BOLD),
                ),
        );

        for tag in analysis.tags.iter().take(6) {
            detail.push(Line::from(format!("• {}", tag.name)));
        }
    }

    frame.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" DECK DETAILS ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            ),
        rows[0],
    );

    let notes = vec![
        Line::from(format!("Status: {}", app.status)),
        Line::from(""),
        Line::from(format!(
            "Sources: {}",
            app.deck_sources.len()
        )),
        Line::from(format!(
            "AI Agent: {}",
            app.deep_learn_status
        )),
        Line::from(""),
        Line::from("Enter / mouse operates active page"),
        Line::from("H/L changes tab"),
        Line::from("Esc returns to deck library"),
    ];

    frame.render_widget(
        Paragraph::new(notes)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" CONTEXT / NOTES ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            ),
        rows[1],
    );
}

fn render_dashboard_bottom_panels(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(62),
            Constraint::Percentage(38),
        ])
        .split(area);

    let mut activity = Vec::<Line>::new();
    activity.push(
        Line::from("● System Ready")
            .style(Style::default().fg(theme::text())),
    );
    activity.push(
        Line::from(format!("  {}", app.status))
            .style(Style::default().fg(theme::muted_color())),
    );

    if let Some(progress) = app.generation_progress.as_ref() {
        activity.push(
            Line::from(format!(
                "  generation {}% • {}",
                progress.percent,
                progress.step,
            ))
            .style(Style::default().fg(theme::yellow())),
        );
    }

    if !app.deep_learn_trace.is_empty() {
        if let Some(last) = app.deep_learn_trace.last() {
            activity.push(
                Line::from(format!("  AI: {last}"))
                    .style(Style::default().fg(theme::muted_color())),
            );
        }
    }

    frame.render_widget(
        Paragraph::new(activity)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" PROGRESS & ACTIVITY ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            ),
        cols[0],
    );

    let action_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(cols[1]);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(action_rows[0]);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(action_rows[1]);

    render_button(
        frame,
        top[0],
        " SIMULATE ",
        UiAction::Simulate,
        theme::cyan(),
        app,
    );
    render_button(
        frame,
        top[1],
        " COMBOS ",
        UiAction::Combos,
        theme::purple(),
        app,
    );
    render_button(
        frame,
        bottom[0],
        " AI AGENT ",
        UiAction::RunDeepLearn,
        theme::magenta(),
        app,
    );
    render_button(
        frame,
        bottom[1],
        " AGENT CHAT ",
        UiAction::OpenAgentChatPage,
        theme::green(),
        app,
    );
}

fn render_mpc(frame: &mut Frame, area: Rect, app: &mut App) {
    app.ensure_mpc_order();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(area);

    // --- Action buttons -----------------------------------------------------
    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 7),
            Constraint::Ratio(1, 7),
            Constraint::Ratio(1, 7),
            Constraint::Ratio(1, 7),
            Constraint::Ratio(1, 7),
            Constraint::Ratio(1, 7),
            Constraint::Ratio(1, 7),
        ])
        .split(rows[0]);

    let searching = app.mpc_progress.is_some();
    render_button(
        frame,
        buttons[0],
        if searching { " SEARCHING… " } else { " SEARCH " },
        UiAction::MpcSearch,
        theme::cyan(),
        app,
    );
    render_button(frame, buttons[1], " ◀ ART ", UiAction::MpcPrevArt, theme::blue(), app);
    render_button(frame, buttons[2], " ART ▶ ", UiAction::MpcNextArt, theme::blue(), app);
    let foil_label = if app.mpc_config.foil { " FOIL: ON " } else { " FOIL: OFF " };
    render_button(frame, buttons[3], foil_label, UiAction::MpcToggleFoil, theme::magenta(), app);
    render_button(frame, buttons[4], " STOCK ", UiAction::MpcCycleStock, theme::purple(), app);
    render_button(frame, buttons[5], " EXPORT XML ", UiAction::MpcExport, theme::green(), app);
    render_button(frame, buttons[6], " MOXFIELD ", UiAction::ExportMoxfield, theme::orange(), app);

    // --- Summary ---------------------------------------------------------
    let summary = match app.mpc_order.as_ref() {
        Some(order) => {
            let (resolved, total) = order.resolved_faces();
            vec![
                Line::from(format!(
                    "{} • {} slots → MPC bracket {} • art {}/{} faces",
                    order.deck_name,
                    order.total_slots(),
                    order.bracket(),
                    resolved,
                    total,
                ))
                .style(Style::default().fg(theme::text()).add_modifier(Modifier::BOLD)),
                Line::from(format!(
                    "Stock {} • foil {} • sideboard {} • cardback {}",
                    order.stock,
                    if order.foil { "yes" } else { "no" },
                    if app.mpc_config.include_sideboard { "included" } else { "excluded" },
                    if order.cardback_id.is_empty() { "not set" } else { "set" },
                ))
                .style(Style::default().fg(theme::muted_color())),
                Line::from(match app.mpc_progress {
                    Some((done, total)) => format!(
                        "Searching {} … {done}/{total}",
                        app.mpc_config.backend_url
                    ),
                    None => format!(
                        "Backend {} • min {} DPI • edit data/mpc_config.json for sources/cardback",
                        app.mpc_config.backend_url, app.mpc_config.minimum_dpi
                    ),
                })
                .style(Style::default().fg(theme::cyan())),
            ]
        }
        None => vec![Line::from(
            "Analyze a deck first; the MPC tab turns it into a MakePlayingCards proxy order.",
        )],
    };

    frame.render_widget(
        Paragraph::new(summary).wrap(Wrap { trim: true }).block(
            Block::default()
                .title(" MPC AUTOFILL ")
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::cyan()))
                .style(theme::panel()),
        ),
        rows[1],
    );

    // --- Card list --------------------------------------------------------
    let list_area = rows[2];
    let inner_height = list_area.height.saturating_sub(2) as usize;
    let mut lines = Vec::<Line>::new();

    if let Some(order) = app.mpc_order.as_ref() {
        let start = app.mpc_scroll.min(order.cards.len().saturating_sub(1));
        for (offset, card) in order.cards.iter().enumerate().skip(start).take(inner_height) {
            let face = |f: &crate::mpc::MpcFace| match f.chosen() {
                Some(image) => format!(
                    "{} {}dpi {}/{}",
                    if image.source_name.is_empty() { "?" } else { image.source_name.as_str() },
                    image.dpi,
                    f.selected + 1,
                    f.candidates.len()
                ),
                None => "— no art —".to_string(),
            };

            let mut text = format!("{:>2}× {:<34.34} {}", card.quantity, card.name, face(&card.front));
            if let Some(back) = &card.back {
                text.push_str(&format!("  ⟲ {}", face(back)));
            }

            let missing = card.front.chosen().is_none()
                || card.back.as_ref().map(|b| b.chosen().is_none()).unwrap_or(false);

            let style = if offset == app.mpc_focus {
                theme::selected()
            } else if missing {
                Style::default().fg(theme::muted_color())
            } else {
                Style::default().fg(theme::text())
            };

            let y = list_area.y + 1 + (offset - start) as u16;
            app.regions.mpc_rows.push((
                offset,
                Rect::new(list_area.x + 1, y, list_area.width.saturating_sub(2), 1),
            ));
            lines.push(Line::from(text).style(style));
        }
    }

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(if app.mpc_show_back { " ORDER • showing BACK faces " } else { " ORDER " })
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::blue()))
                .style(theme::panel()),
        ),
        list_area,
    );

    // --- Missing ------------------------------------------------------------
    let footer = if app.mpc_not_found.is_empty() {
        "Exports land in ./exports: <deck>.mpc.xml (desktop tool / mpcfill.com import) and <deck>.mpc.txt (paste into mpcfill.com).".to_string()
    } else {
        format!(
            "No MPC art for {}: {}",
            app.mpc_not_found.len(),
            app.mpc_not_found.iter().take(10).cloned().collect::<Vec<_>>().join(", ")
        )
    };

    frame.render_widget(
        Paragraph::new(footer)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(theme::muted_color()))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::muted_color()))
                    .style(theme::panel()),
            ),
        rows[3],
    );
}

fn render_dashboard_key_strip(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    frame.render_widget(
        Paragraph::new(format!(
            "{}     {}",
            app.page.title(),
            if app.page == Page::Mpc {
                "↑/↓ card   [ ] art   V front/back   F search   X export order   T foil   O stock   B sideboard   M Moxfield   H/L tabs"
            } else {
                "H/L tabs     ↑/↓ navigate     Enter select     S simulate     C combos     M Moxfield export     Q quit"
            },
        ))
        .style(
            Style::default()
                .fg(theme::cyan())
                .bg(theme::bg()),
        ),
        area,
    );
}

fn render_dashboard_compact(frame: &mut Frame, area: Rect, app: &mut App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(10),
        ])
        .split(area);

    let subtitle = dashboard_subtitle(app);

    render_banner(
        frame,
        rows[0],
        "COMMANDER LAB",
        &subtitle,
    );

    match app.page {
        Page::Overview => render_overview(frame, rows[1], app),
        Page::Cards => render_cards(frame, rows[1], app),
        Page::Mana => render_mana(frame, rows[1], app),
        Page::Curve => render_curve(frame, rows[1], app),
        Page::Roles => render_roles(frame, rows[1], app),
        Page::Legality => render_legality(frame, rows[1], app),
        Page::Suggestions => render_suggestions(frame, rows[1], app),
        Page::Bracket => render_bracket(frame, rows[1], app),
        Page::Simulation => render_simulation(frame, rows[1], app),
        Page::Combos => render_combos(frame, rows[1], app),
        Page::Sources => render_sources(frame, rows[1], app),
        Page::Mpc => render_mpc(frame, rows[1], app),
        Page::Log => render_deck_log(frame, rows[1], app),
        Page::Help => render_help(frame, rows[1]),
    }
}

fn dashboard_subtitle(app: &App) -> String {
    app.analysis
        .as_ref()
        .map(|analysis| {
            format!(
                "{} • {} • Bracket {} • {}",
                analysis.deck_name,
                pretty_identity(&analysis.identity),
                analysis.bracket.bracket,
                format_deck_price(
                    analysis_price_values(analysis).0,
                ),
            )
        })
        .unwrap_or_else(|| "No analysis".to_string())
}



fn render_banner(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    subtitle: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(24),
            Constraint::Min(1),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(title)
            .alignment(Alignment::Center)
            .style(theme::title())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::orange()))
                    .style(theme::panel()),
            ),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(subtitle)
            .style(
                Style::default()
                    .fg(theme::muted_color())
                    .bg(theme::panel_color()),
            )
            .block(
                Block::default()
                    .title(" ACTIVE ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            ),
        chunks[1],
    );
}

fn render_clickable_tabs(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(area);

    let first = &Page::ALL[..7];
    let second = &Page::ALL[7..];

    render_tab_row(frame, rows[0], app, first);
    render_tab_row(frame, rows[1], app, second);
}

fn render_tab_row(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
    pages: &[Page],
) {
    let constraints = pages
        .iter()
        .map(|_| Constraint::Ratio(1, pages.len() as u32))
        .collect::<Vec<_>>();

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (page, rect) in pages.iter().zip(chunks.iter()) {
        app.regions.tabs.push((*page, *rect));

        frame.render_widget(
            Paragraph::new(
                if *page == app.page {
                    format!("› {} ‹", page.title())
                } else {
                    page.title().to_string()
                }
            )
            .alignment(Alignment::Center)
            .style(
                if *page == app.page {
                    theme::selected()
                } else {
                    Style::default()
                        .fg(theme::muted_color())
                        .bg(theme::bg())
                }
            ),
            *rect,
        );
    }
}

fn render_overview(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(analysis) = app.analysis.clone() else {
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(14),
        ])
        .split(area);

    let tiles = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(17),
            Constraint::Percentage(17),
            Constraint::Percentage(16),
            Constraint::Percentage(16),
            Constraint::Percentage(17),
            Constraint::Percentage(17),
        ])
        .split(rows[0]);

    render_tile(
        frame,
        tiles[0],
        "LEGALITY",
        if analysis.legal {
            "LEGAL".to_string()
        } else {
            "CHECK ISSUES".to_string()
        },
        if analysis.legal {
            theme::green()
        } else {
            theme::red()
        },
    );

    render_tile(
        frame,
        tiles[1],
        "BRACKET",
        analysis.bracket.display_name(),
        bracket_color(analysis.bracket.bracket),
    );

    render_tile(
        frame,
        tiles[2],
        "LANDS",
        analysis.types.lands.to_string(),
        theme::blue(),
    );

    render_tile(
        frame,
        tiles[3],
        "AVG MANA VALUE",
        format!("{:.2}", analysis.avg_cmc),
        theme::magenta(),
    );

    render_tile(
        frame,
        tiles[4],
        "SIDEBOARD",
        analysis.sideboard_count.to_string(),
        theme::cyan(),
    );

    render_tile(
        frame,
        tiles[5],
        "DECK PRICE",
        {
            let (deck_price, sideboard_price) =
                analysis_price_values(&analysis);
            format!(
                "{} • SB {}\n{} known • {} unpriced",
                format_deck_price(deck_price),
                format_deck_price(sideboard_price),
                analysis.known_cards,
                analysis.unpriced_cards,
            )
        },
        theme::yellow(),
    );

    // Keep the compact statistics on the left so the card browser and image
    // preview can use the full vertical height of the Overview workspace.
    let overview_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ])
        .split(rows[1]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Min(6),
        ])
        .split(overview_columns[0]);

    render_type_chart(frame, left[0], &analysis);
    render_role_chart(frame, left[1], &analysis);

    let bottom = [left[2], overview_columns[1], overview_columns[2]];

    let tags = if analysis.tags.is_empty() {
        "No dominant archetype tags detected.".to_string()
    } else {
        analysis
            .tags
            .iter()
            .map(|tag| {
                format!(
                    "{:<16} score {:>3}  matches {}",
                    tag.name, tag.score, tag.matched_cards
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    frame.render_widget(
        Paragraph::new(tags)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" EDHREC-STYLE ARCHETYPE TAGS — CLICK ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::purple()))
                    .style(theme::panel()),
            ),
        bottom[0],
    );

    for (index, tag) in analysis
        .tags
        .iter()
        .take(bottom[0].height.saturating_sub(2) as usize)
        .enumerate()
    {
        app.regions.archetype_tags.push((
            tag.name.clone(),
            Rect::new(
                bottom[0].x + 1,
                bottom[0].y + 1 + index as u16,
                bottom[0].width.saturating_sub(2),
                1,
            ),
        ));
    }



    if let Some(tag) = app.selected_archetype_tag.clone() {
        let cards = app.cards_for_archetype(&tag);
        render_named_card_list(
            frame,
            bottom[1],
            &format!(" {tag} CARDS "),
            &cards,
            app,
        );
    } else {
        let preview_cards = app
            .parsed
            .as_ref()
            .map(|deck| {
                deck.commanders
                    .iter()
                    .chain(deck.main.iter())
                    .take(18)
                    .map(|entry| entry.name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        render_named_card_list(
            frame,
            bottom[1],
            " DECK CARD PREVIEW • HOVER / CLICK ",
            &preview_cards,
            app,
        );
    }

    render_card_preview_panel(frame, bottom[2], app);
}

fn render_cards(frame: &mut Frame, area: Rect, app: &mut App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
        ])
        .split(area);

    let zone_buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(18),
            Constraint::Length(18),
            Constraint::Min(1),
        ])
        .split(rows[0]);

    render_button(
        frame,
        zone_buttons[0],
        &format!(
            " MAINBOARD {} ",
            app.parsed
                .as_ref()
                .map(|deck| {
                    deck.commanders
                        .iter()
                        .chain(deck.main.iter())
                        .map(|entry| entry.quantity as usize)
                        .sum::<usize>()
                })
                .unwrap_or(0)
        ),
        UiAction::Mainboard,
        if app.card_zone == CardZone::Mainboard {
            theme::blue()
        } else {
            theme::panel_alt_color()
        },
        app,
    );

    render_button(
        frame,
        zone_buttons[1],
        &format!(" SIDEBOARD {} ", app.sideboard_count()),
        UiAction::Sideboard,
        if app.card_zone == CardZone::Sideboard {
            theme::magenta()
        } else {
            theme::panel_alt_color()
        },
        app,
    );

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(38),
            Constraint::Percentage(62),
        ])
        .split(rows[1]);

    app.regions.card_list = Some(cols[0]);

    let entries = app.card_entries();
    let visible_height = cols[0].height.saturating_sub(2) as usize;
    let end = (app.card_scroll + visible_height).min(entries.len());

    let visible = if app.card_scroll < end {
        &entries[app.card_scroll..end]
    } else {
        &[]
    };

    let items = visible
        .iter()
        .map(|(name, quantity, commander)| {
            let prefix = if *commander { "★" } else { " " };
            let price = card_short_price(app, name);
            ListItem::new(format!(
                "{prefix} {quantity}x {name}   {price}"
            ))
        })
        .collect::<Vec<_>>();

    let mut state = ListState::default();

    if !visible.is_empty()
        && app.selected_card >= app.card_scroll
        && app.selected_card < end
    {
        state.select(Some(app.selected_card - app.card_scroll));
    }

    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" CARDS — CLICK / WHEEL ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            )
            .highlight_symbol("▌ ")
            .highlight_style(theme::selected()),
        cols[0],
        &mut state,
    );

    for (visible_index, (name, _, _)) in visible.iter().enumerate() {
        let y = cols[0].y + 1 + visible_index as u16;
        let rect = Rect::new(
            cols[0].x + 1,
            y,
            cols[0].width.saturating_sub(2),
            1,
        );
        app.regions.named_cards.push((name.clone(), rect));
    }

    render_card_preview_panel(frame, cols[1], app);
}

fn render_mana(frame: &mut Frame, area: Rect, app: &App) {
    let Some(analysis) = app.analysis.as_ref() else {
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(rows[0]);

    let source_group = BarGroup::new([
        Bar::with_label("W", analysis.direct_sources[0])
            .style(Style::default().fg(theme::white_mana())),
        Bar::with_label("U", analysis.direct_sources[1])
            .style(Style::default().fg(theme::blue_mana())),
        Bar::with_label("B", analysis.direct_sources[2])
            .style(Style::default().fg(theme::black_mana())),
        Bar::with_label("R", analysis.direct_sources[3])
            .style(Style::default().fg(theme::red_mana())),
        Bar::with_label("G", analysis.direct_sources[4])
            .style(Style::default().fg(theme::green_mana())),
    ]);

    frame.render_widget(
        BarChart::default()
            .block(
                Block::default()
                    .title(" DIRECT COLORED SOURCES ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            )
            .data(source_group)
            .bar_width(5)
            .bar_gap(2)
            .value_style(
                Style::default()
                    .fg(theme::text())
                    .add_modifier(Modifier::BOLD),
            )
            .label_style(Style::default().fg(theme::text())),
        top[0],
    );

    frame.render_widget(
        Paragraph::new(format!(
            "LANDS             {}\nFETCH LANDS        {}\nAVG MANA VALUE     {:.2}\nSIDEBOARD          {}\n\nDirect-source bars use Scryfall produced_mana. Fetch lands are tracked separately.",
            analysis.types.lands,
            analysis.fetch_lands,
            analysis.avg_cmc,
            analysis.sideboard_count,
        ))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" SOURCE NOTES ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::cyan()))
                .style(theme::panel()),
        ),
        top[1],
    );

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(rows[1]);

    let pip_group = BarGroup::new([
        Bar::with_label("W", analysis.pip_demand[0])
            .style(Style::default().fg(theme::white_mana())),
        Bar::with_label("U", analysis.pip_demand[1])
            .style(Style::default().fg(theme::blue_mana())),
        Bar::with_label("B", analysis.pip_demand[2])
            .style(Style::default().fg(theme::black_mana())),
        Bar::with_label("R", analysis.pip_demand[3])
            .style(Style::default().fg(theme::red_mana())),
        Bar::with_label("G", analysis.pip_demand[4])
            .style(Style::default().fg(theme::green_mana())),
    ]);

    frame.render_widget(
        BarChart::default()
            .block(
                Block::default()
                    .title(" COLORED PIP DEMAND ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::magenta()))
                    .style(theme::panel()),
            )
            .data(pip_group)
            .bar_width(5)
            .bar_gap(2)
            .value_style(
                Style::default()
                    .fg(theme::text())
                    .add_modifier(Modifier::BOLD),
            )
            .label_style(Style::default().fg(theme::text())),
        bottom[0],
    );

    let demand_total: u64 = analysis.pip_demand.iter().sum();
    let source_total: u64 = analysis.direct_sources.iter().sum();
    let colors = ["WHITE", "BLUE", "BLACK", "RED", "GREEN"];

    let mut lines = Vec::new();

    for index in 0..5 {
        let demand = if demand_total == 0 {
            0.0
        } else {
            analysis.pip_demand[index] as f64
                / demand_total as f64
                * 100.0
        };

        let supply = if source_total == 0 {
            0.0
        } else {
            analysis.direct_sources[index] as f64
                / source_total as f64
                * 100.0
        };

        lines.push(format!(
            "{:<6} demand {:>5.1}%   source {:>5.1}%",
            colors[index], demand, supply
        ));
    }

    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .block(
                Block::default()
                    .title(" DEMAND vs SUPPLY SHARE ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::yellow()))
                    .style(theme::panel()),
            ),
        bottom[1],
    );
}

fn render_curve(frame: &mut Frame, area: Rect, app: &App) {
    let Some(analysis) = app.analysis.as_ref() else {
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(72),
            Constraint::Length(5),
            Constraint::Min(3),
        ])
        .split(area);

    let curve_points = analysis
        .curve
        .iter()
        .enumerate()
        .map(|(x, y)| (x as f64, *y as f64))
        .collect::<Vec<_>>();

    let max_curve =
        analysis.curve.iter().copied().max().unwrap_or(1).max(1);

    let datasets = vec![
        Dataset::default()
            .name("Spells")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(
                Style::default()
                    .fg(theme::cyan())
                    .add_modifier(Modifier::BOLD),
            )
            .data(&curve_points),
    ];

    frame.render_widget(
        Chart::new(datasets)
            .block(
                Block::default()
                    .title(" MANA CURVE ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            )
            .x_axis(
                Axis::default()
                    .title("Mana value")
                    .style(Style::default().fg(theme::muted_color()))
                    .bounds([0.0, 7.0])
                    .labels([
                        "0", "1", "2", "3", "4", "5", "6", "7+",
                    ]),
            )
            .y_axis(
                Axis::default()
                    .title("Cards")
                    .style(Style::default().fg(theme::muted_color()))
                    .bounds([0.0, max_curve as f64])
                    .labels([
                        Line::from("0"),
                        Line::from((max_curve / 2).max(1).to_string()),
                        Line::from(max_curve.to_string()),
                    ]),
            ),
        rows[0],
    );

    frame.render_widget(
        Sparkline::default()
            .block(
                Block::default()
                    .title(" CURVE SPARKLINE ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::magenta()))
                    .style(theme::panel()),
            )
            .data(&analysis.curve)
            .max(max_curve)
            .style(Style::default().fg(theme::magenta())),
        rows[1],
    );

    let high =
        analysis.curve[5] + analysis.curve[6] + analysis.curve[7];
    let cheap =
        analysis.curve[0] + analysis.curve[1] + analysis.curve[2];

    render_tile(
        frame,
        rows[2],
        "CURVE READ",
        format!(
            "Average MV {:.2}   •   MV 0–2: {}   •   MV 5+: {}",
            analysis.avg_cmc, cheap, high
        ),
        if analysis.avg_cmc <= 3.2 {
            theme::green()
        } else {
            theme::orange()
        },
    );
}

fn render_roles(frame: &mut Frame, area: Rect, app: &App) {
    let Some(analysis) = app.analysis.as_ref() else {
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(4),
        ])
        .split(area);

    render_role_line_gauge(
        frame,
        rows[0],
        "RAMP",
        analysis.roles.ramp,
        12,
        theme::green(),
    );

    render_role_line_gauge(
        frame,
        rows[1],
        "DRAW",
        analysis.roles.draw,
        12,
        theme::cyan(),
    );

    render_role_line_gauge(
        frame,
        rows[2],
        "INTERACTION",
        analysis.roles.removal + analysis.roles.counters,
        12,
        theme::orange(),
    );

    render_role_line_gauge(
        frame,
        rows[3],
        "TUTORS",
        analysis.roles.tutors,
        8,
        theme::magenta(),
    );

    render_role_line_gauge(
        frame,
        rows[4],
        "BOARD WIPES",
        analysis.roles.wipes,
        4,
        theme::red(),
    );

    render_role_line_gauge(
        frame,
        rows[5],
        "RECURSION",
        analysis.roles.recursion,
        8,
        theme::purple(),
    );

    render_role_line_gauge(
        frame,
        rows[6],
        "PROTECTION",
        analysis.roles.protection,
        8,
        theme::yellow(),
    );

    let role_profile = [
        analysis.roles.ramp as u64,
        analysis.roles.draw as u64,
        (analysis.roles.removal + analysis.roles.counters) as u64,
        analysis.roles.tutors as u64,
        analysis.roles.wipes as u64,
        analysis.roles.recursion as u64,
        analysis.roles.protection as u64,
        analysis.roles.graveyard as u64,
    ];

    frame.render_widget(
        Sparkline::default()
            .block(
                Block::default()
                    .title(" ROLE PROFILE ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            )
            .data(&role_profile)
            .max(
                role_profile
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(1)
                    .max(1),
            )
            .style(Style::default().fg(theme::blue())),
        rows[7],
    );
}

fn render_legality(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(analysis) = app.analysis.clone() else {
        return;
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28),
            Constraint::Percentage(42),
            Constraint::Percentage(30),
        ])
        .split(area);

    let mut cards = analysis.commanders.clone();
    for card in &analysis.unknown_cards {
        if !cards.iter().any(|name| name == card) {
            cards.push(card.clone());
        }
    }

    render_named_card_list(
        frame,
        cols[0],
        " REFERENCED CARDS ",
        &cards,
        app,
    );

    let issues = if analysis.legality_issues.is_empty() {
        "No Commander construction problems detected.".to_string()
    } else {
        analysis
            .legality_issues
            .iter()
            .enumerate()
            .map(|(index, issue)| {
                format!("{}. {}", index + 1, issue)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    frame.render_widget(
        Paragraph::new(issues.clone())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(if analysis.legal {
                        " LEGALITY — LEGAL "
                    } else {
                        " LEGALITY — ISSUES "
                    })
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(if analysis.legal {
                        theme::green()
                    } else {
                        theme::red()
                    }))
                    .style(theme::panel()),
            ),
        cols[1],
    );

    let issue_lines = issues.lines().map(str::to_string).collect::<Vec<_>>();
    register_text_card_hits(app, cols[1], &issue_lines);

    render_card_preview_panel(frame, cols[2], app);
}

fn render_suggestions(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(analysis) = app.analysis.clone() else {
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(34),
            Constraint::Percentage(32),
        ])
        .split(rows[0]);

    let cuts = analysis
        .cut_candidates
        .iter()
        .map(|candidate| {
            (
                candidate.card.clone(),
                format!(
                    "score {} • tag fit {}\n{}",
                    candidate.score,
                    candidate.tag_fit,
                    candidate.reason
                ),
            )
        })
        .collect::<Vec<_>>();

    let adds = analysis
        .add_suggestions
        .iter()
        .map(|suggestion| {
            (
                suggestion.card.clone(),
                format!(
                    "{} • tag fit {} • EDHREC rank {}",
                    suggestion.role,
                    suggestion.tag_fit,
                    suggestion
                        .edhrec_rank
                        .map(|rank| rank.to_string())
                        .unwrap_or_else(|| "-".to_string())
                ),
            )
        })
        .collect::<Vec<_>>();

    render_suggestion_detail_list(
        frame,
        cols[0],
        " CUT CANDIDATES — CLICK ",
        &cuts,
        SuggestionKind::Cut,
        theme::orange(),
        app.suggestion_focus_column == 0,
        app.suggestion_focus_index,
        app,
    );

    render_suggestion_detail_list(
        frame,
        cols[1],
        " ARCHETYPE-AWARE ADDS — CLICK ",
        &adds,
        SuggestionKind::Add,
        theme::green(),
        app.suggestion_focus_column == 1,
        app.suggestion_focus_index,
        app,
    );

    render_card_preview_panel(frame, cols[2], app);

    let controls = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),
            Constraint::Min(1),
        ])
        .split(rows[1]);

    render_button(
        frame,
        controls[0],
        " SIDEBOARD SELECTED ",
        UiAction::SideboardSuggestion,
        theme::purple(),
        app,
    );

    let selected = app
        .selected_suggestion
        .as_ref()
        .map(|selection| {
            format!(
                "{}: {}",
                match selection.kind {
                    SuggestionKind::Cut => "CUT",
                    SuggestionKind::Add => "ADD",
                },
                selection.card
            )
        })
        .unwrap_or_else(|| {
            "Keyboard: ←/→ column • ↑/↓ row • Enter preview/select • B sideboard"
                .to_string()
        });

    frame.render_widget(
        Paragraph::new(selected)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::muted_color())),
        controls[1],
    );
}

fn render_bracket(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(analysis) = app.analysis.clone() else {
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(7),
        ])
        .split(area);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(rows[0]);

    render_tile(
        frame,
        top[0],
        "ESTIMATED BRACKET",
        analysis.bracket.display_name(),
        bracket_color(analysis.bracket.bracket),
    );

    render_tile(
        frame,
        top[1],
        "GAME CHANGERS",
        analysis.bracket.game_changers.len().to_string(),
        theme::magenta(),
    );

    render_tile(
        frame,
        top[2],
        "FAST MANA",
        analysis.bracket.fast_mana.to_string(),
        theme::cyan(),
    );

    render_tile(
        frame,
        top[3],
        "TUTORS",
        analysis.bracket.tutors.to_string(),
        theme::orange(),
    );

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28),
            Constraint::Percentage(42),
            Constraint::Percentage(30),
        ])
        .split(rows[1]);

    render_named_card_list(
        frame,
        bottom[0],
        " GAME CHANGERS ",
        &analysis.bracket.game_changers,
        app,
    );

    let mut reasons = analysis.bracket.reasons.join("\n");
    reasons.push_str("\n\n");
    reasons.push_str(&analysis.bracket.confidence_note);

    frame.render_widget(
        Paragraph::new(reasons.clone())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" WHY THIS ESTIMATE ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::yellow()))
                    .style(theme::panel()),
            ),
        bottom[1],
    );

    let reason_lines = reasons.lines().map(str::to_string).collect::<Vec<_>>();
    register_text_card_hits(app, bottom[1], &reason_lines);

    let bracket_pressure = format!(
        "Extra-turn cards: {}  •  Mass-land-denial cards: {}",
        analysis.bracket.extra_turn_cards,
        analysis.bracket.mass_land_denial,
    );

    frame.render_widget(
        Paragraph::new(bracket_pressure)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::muted_color())),
        Rect::new(
            area.x + 1,
            area.y + area.height.saturating_sub(2),
            area.width.saturating_sub(2),
            1,
        ),
    );

    render_card_preview_panel(frame, bottom[2], app);
}

fn render_simulation(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(simulation) = app.simulation.as_ref() else {
        frame.render_widget(
            Paragraph::new(
                "Click SIMULATE or press S/Enter. Afterward use ↑/↓ to keyboard-highlight each consistency metric.",
            )
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" MONTE CARLO ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::cyan()))
                    .style(theme::panel()),
            ),
            area,
        );
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(4),
        ])
        .split(area);

    render_tile(
        frame,
        rows[0],
        "SIMULATION",
        format!(
            "{} local runs\nAverage opening lands: {:.2}",
            simulation.runs,
            simulation.average_opening_lands
        ),
        theme::blue(),
    );

    let profile = [
        simulation.keepable_opening_pct.round() as u64,
        simulation.three_lands_by_t3_pct.round() as u64,
        simulation.four_lands_by_t4_pct.round() as u64,
        simulation.five_lands_by_t5_pct.round() as u64,
        simulation.early_ramp_by_t2_pct.round() as u64,
    ];

    frame.render_widget(
        Sparkline::default()
            .block(
                Block::default()
                    .title(" CONSISTENCY PROFILE ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::magenta()))
                    .style(theme::panel()),
            )
            .data(&profile)
            .max(100)
            .style(Style::default().fg(theme::magenta())),
        rows[1],
    );

    render_pct_gauge(
        frame,
        rows[2],
        "OPENING 2–5 LANDS",
        simulation.keepable_opening_pct,
        theme::green(),
        app.simulation_focus_index == 0,
    );

    render_pct_gauge(
        frame,
        rows[3],
        "3+ LANDS BY TURN 3",
        simulation.three_lands_by_t3_pct,
        theme::cyan(),
        app.simulation_focus_index == 1,
    );

    render_pct_gauge(
        frame,
        rows[4],
        "4+ LANDS BY TURN 4",
        simulation.four_lands_by_t4_pct,
        theme::blue(),
        app.simulation_focus_index == 2,
    );

    render_pct_gauge(
        frame,
        rows[5],
        "5+ LANDS BY TURN 5",
        simulation.five_lands_by_t5_pct,
        theme::magenta(),
        app.simulation_focus_index == 3,
    );

    render_pct_gauge(
        frame,
        rows[6],
        "CMC ≤2 RAMP BY TURN 2",
        simulation.early_ramp_by_t2_pct,
        theme::yellow(),
        app.simulation_focus_index == 4,
    );
}

fn render_combos(frame: &mut Frame, area: Rect, app: &mut App) {
    let Some(report) = app.combos.clone() else {
        frame.render_widget(
            Paragraph::new(
                "Press C to query Commander Spellbook. Complete and almost-complete variants become selectable here.",
            )
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" COMMANDER SPELLBOOK ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::magenta()))
                    .style(theme::panel()),
            ),
            area,
        );
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(area);

    let stats = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(rows[0]);

    render_tile(frame, stats[0], "COMPLETE COMBOS", report.included.to_string(), theme::green());
    render_tile(frame, stats[1], "ALMOST INCLUDED", report.almost_included.to_string(), theme::yellow());

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(36),
            Constraint::Percentage(42),
            Constraint::Percentage(22),
        ])
        .split(rows[1]);

    let mut lines = Vec::<Line>::new();
    let mut y = cols[0].y + 1;
    let max_y = cols[0].y + cols[0].height.saturating_sub(1);

    for (index, combo) in report.variants.iter().enumerate() {
        if y >= max_y {
            break;
        }

        let marker = if combo.almost { "+1" } else { "✓" };

        lines.push(
            Line::from(format!("{} {}", marker, combo.cards.join(" + ")))
                .style(if index == app.combo_focus_index {
                    theme::selected()
                } else {
                    Style::default().fg(theme::text())
                }),
        );

        app.regions.combo_rows.push((
            index,
            Rect::new(
                cols[0].x + 1,
                y,
                cols[0].width.saturating_sub(2),
                1,
            ),
        ));

        register_card_names_in_row(
            app,
            cols[0],
            y,
            &combo.cards,
        );

        y += 1;
    }

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" COMBOS — HOVER / ↑↓ ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::magenta()))
                    .style(theme::panel()),
            ),
        cols[0],
    );

    if let Some(combo) = report.variants.get(app.combo_focus_index) {
        let mut detail = vec![
            Line::from(if combo.almost {
                "ALMOST INCLUDED"
            } else {
                "COMPLETE IN DECK"
            })
            .style(
                Style::default()
                    .fg(if combo.almost { theme::yellow() } else { theme::green() })
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            Line::from("Cards")
                .style(
                    Style::default()
                        .fg(theme::magenta())
                        .add_modifier(Modifier::BOLD),
                ),
        ];

        for card_name in &combo.cards {
            let row_index = detail.len();
            let prefix = "• ";
            detail.push(
                Line::from(format!("{prefix}{card_name}"))
                    .style(Style::default().fg(theme::text())),
            );

            register_exact_card_hits(
                app,
                cols[1],
                cols[1]
                    .y
                    .saturating_add(1)
                    .saturating_add(row_index as u16),
                prefix,
                std::slice::from_ref(card_name),
            );
        }

        if !combo.result.is_empty() {
            detail.push(Line::from(format!("Result: {}", combo.result)));
        }

        if !combo.initial_state.is_empty() {
            detail.push(Line::from(""));
            detail.push(Line::from("Initial card state").style(Style::default().fg(theme::orange())));
            for item in &combo.initial_state {
                detail.push(Line::from(format!("• {item}")));
            }
        }

        if !combo.prerequisites.is_empty() {
            detail.push(Line::from(""));
            detail.push(Line::from("Prerequisites").style(Style::default().fg(theme::yellow())));
            for item in &combo.prerequisites {
                detail.push(Line::from(format!("• {item}")));
            }
        }

        if !combo.steps.is_empty() {
            detail.push(Line::from(""));
            detail.push(Line::from("Spellbook breakdown").style(Style::default().fg(theme::cyan())));
            for (index, step) in combo.steps.iter().enumerate() {
                detail.push(Line::from(format!("{}. {}", index + 1, step)));
            }
        }

        detail.push(Line::from(""));
        detail.push(Line::from(combo.spellbook_url.clone()).style(Style::default().fg(theme::cyan())));

        frame.render_widget(
            Paragraph::new(detail)
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .title(" COMMANDER SPELLBOOK BREAKDOWN ")
                        .borders(Borders::ALL)
                    .border_type(theme::border_type())
                        .border_style(Style::default().fg(theme::cyan()))
                        .style(theme::panel()),
                ),
            cols[1],
        );
    }

    render_card_preview_panel(frame, cols[2], app);

    let controls = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),
            Constraint::Min(1),
        ])
        .split(rows[2]);

    render_button(
        frame,
        controls[0],
        " OPEN SPELLBOOK ",
        UiAction::OpenSelectedCombo,
        theme::cyan(),
        app,
    );

    frame.render_widget(
        Paragraph::new("Hover or ↑/↓ selects • Enter previews first card • O opens Spellbook")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::muted_color())),
        controls[1],
    );
}

fn render_deck_log(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let inner_height =
        area.height.saturating_sub(2) as usize;
    let start = app.deck_log_scroll as usize;
    let end = (start + inner_height)
        .min(app.deck_history_lines.len());

    let mut lines = Vec::<Line>::new();

    for (screen_offset, index) in (start..end).enumerate() {
        let hovered = app.hovered_text
            == Some((TextPane::Log, index));

        lines.push(
            Line::from(
                app.deck_history_lines[index].clone(),
            )
            .style(
                if hovered {
                    theme::selected()
                } else {
                    Style::default().fg(theme::text())
                },
            ),
        );

        app.regions.text_rows.push((
            TextPane::Log,
            index,
            Rect::new(
                area.x + 1,
                area.y + 1 + screen_offset as u16,
                area.width.saturating_sub(2),
                1,
            ),
        ));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" DECK TEST / AGENT HISTORY • WHEEL / PGUP / PGDN • HOVER HIGHLIGHTS ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::purple()))
                    .style(theme::panel()),
            ),
        area,
    );
}

fn render_sources(frame: &mut Frame, area: Rect, app: &mut App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(8),
        ])
        .split(area);

    let user_count = app
        .deck_sources
        .iter()
        .filter(|source| source.user_provided)
        .count();

    frame.render_widget(
        Paragraph::new(format!(
            "Automatic research remains active, and you can attach your own high-priority references. \
             Moxfield/Archidekt links are imported as full decklists; other public URLs are cached as strategic excerpts. \
             User references attached: {}.",
            user_count,
        ))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" DECK RESEARCH SOURCES ")
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::cyan()))
                .style(theme::panel()),
        ),
        rows[0],
    );

    let controls = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(rows[1]);

    render_button(
        frame,
        controls[0],
        " + ADD USER REFERENCE ",
        UiAction::AddUserReference,
        theme::green(),
        app,
    );

    let selected_is_user = app
        .deck_sources
        .get(app.selected_source)
        .map(|source| source.user_provided)
        .unwrap_or(false);

    render_button(
        frame,
        controls[1],
        if selected_is_user {
            " REMOVE SELECTED USER REFERENCE "
        } else {
            " REMOVE — USER REFERENCES ONLY "
        },
        UiAction::RemoveUserReference,
        if selected_is_user {
            theme::red()
        } else {
            theme::muted_color()
        },
        app,
    );

    let mut lines = Vec::<Line>::new();
    let mut y = rows[2].y + 1;
    let max_y = rows[2].y + rows[2].height.saturating_sub(1);

    for (index, source) in app
        .deck_sources
        .iter()
        .enumerate()
        .skip(app.source_scroll)
    {
        if y >= max_y {
            break;
        }

        let block_y = y;

        let selected = index == app.selected_source;
        let style = if selected {
            theme::selected()
        } else if source.user_provided {
            Style::default()
                .fg(theme::yellow())
                .bg(theme::panel_color())
        } else {
            Style::default()
                .fg(theme::text())
                .bg(theme::panel_color())
        };

        lines.push(
            Line::from(format!(
                "{}. {}[{}] {}{}",
                index + 1,
                if source.user_provided {
                    "★ USER • "
                } else {
                    ""
                },
                source.source,
                source.title,
                if source.user_provided {
                    format!(" • PRIORITY {}", source.priority)
                } else {
                    String::new()
                },
            ))
            .style(style),
        );

        app.regions.source_links.push((
            index,
            source.url.clone(),
            Rect::new(
                rows[2].x + 1,
                y,
                rows[2].width.saturating_sub(2),
                1,
            ),
        ));
        y += 1;

        if y < max_y {
            lines.push(
                Line::from(format!("   {}", source.detail))
                    .style(Style::default().fg(theme::muted_color())),
            );
            y += 1;
        }

        if y < max_y {
            lines.push(
                Line::from(format!("   {}", source.url))
                    .style(Style::default().fg(theme::cyan())),
            );
            y += 1;
        }

        app.regions.source_rows.push((
            index,
            Rect::new(
                rows[2].x + 1,
                block_y,
                rows[2].width.saturating_sub(2),
                y.saturating_sub(block_y).max(1),
            ),
        ));
    }

    if lines.is_empty() {
        lines.push(
            Line::from(
                "No sources attached yet. Add a reference or run AI Agent research.",
            )
            .style(Style::default().fg(theme::muted_color())),
        );
    }

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" SOURCES • WHEEL/↑↓ SCROLL • HOVER HIGHLIGHTS • ENTER OPEN • + ADD • DEL REMOVE USER ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            ),
        rows[2],
    );
}

fn render_deep_learn_screen(frame: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(4),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_app_header(
        frame,
        root[0],
        "AI AGENT",
        app.parsed
            .as_ref()
            .map(|deck| deck.name.as_str())
            .unwrap_or("No deck"),
        app,
    );

    let summary = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(root[1]);

    render_summary_card(
        frame,
        summary[0],
        "MODEL",
        app.deep_learn
            .as_ref()
            .map(|r| format!("{}%", r.confidence))
            .unwrap_or_else(|| "NOT RUN".to_string()),
        "research confidence",
        theme::purple(),
        false,
    );

    render_summary_card(
        frame,
        summary[1],
        "TEST",
        app.deep_learn_intensity.label().to_string(),
        &format!("{} games", app.deep_learn_intensity.goldfish_runs()),
        theme::cyan(),
        false,
    );

    let (speed_title, speed_value, speed_subtitle) =
        if let Some(agent) = app.deep_agent_result.as_ref() {
            if agent.goal == OptimizationGoal::StaxLock {
                (
                    "EARLIEST LOCK",
                    agent
                        .final_stax
                        .as_ref()
                        .and_then(|stax| stax.earliest_lock_turn)
                        .map(|turn| format!("T{turn}"))
                        .unwrap_or_else(|| "-".to_string()),
                    "stax pressure model",
                )
            } else {
                (
                    "EARLIEST WIN",
                    agent
                        .final_testing
                        .goldfish
                        .earliest_combo_turn
                        .map(|turn| format!("T{turn}"))
                        .unwrap_or_else(|| "-".to_string()),
                    "known-line goldfish",
                )
            }
        } else {
            (
                "EARLIEST WIN",
                app.deep_learn_testing
                    .as_ref()
                    .and_then(|t| t.goldfish.earliest_combo_turn)
                    .map(|turn| format!("T{turn}"))
                    .unwrap_or_else(|| "-".to_string()),
                "known-line goldfish",
            )
        };

    render_summary_card(
        frame,
        summary[2],
        speed_title,
        speed_value,
        speed_subtitle,
        theme::green(),
        false,
    );

    render_summary_card(
        frame,
        summary[3],
        "UPGRADE",
        format!("B{}", app.deep_learn_upgrade_bracket),
        "target bracket",
        theme::yellow(),
        false,
    );

    render_deep_learn_tabs(frame, root[2], app);

    let workspace = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(76),
            Constraint::Percentage(24),
        ])
        .split(root[3]);

    match app.deep_learn_page {
        DeepLearnPage::Overview => {
            render_deep_learn_overview(frame, workspace[0], app)
        }
        DeepLearnPage::Agent => {
            render_deep_agent(frame, workspace[0], app)
        }
        DeepLearnPage::Chat => {
            render_deep_agent_chat(frame, workspace[0], app)
        }
        DeepLearnPage::Research => {
            render_deep_learn_research(frame, workspace[0], app)
        }
        DeepLearnPage::Simulate => {
            render_deep_learn_simulate(frame, workspace[0], app)
        }
        DeepLearnPage::WinSpeed => {
            render_deep_learn_win_speed(frame, workspace[0], app)
        }
        DeepLearnPage::Upgrade => {
            render_deep_learn_upgrade(frame, workspace[0], app)
        }
        DeepLearnPage::Recommendations => {
            render_deep_learn_recommendations(frame, workspace[0], app)
        }
        DeepLearnPage::Compare => {
            render_deep_agent_comparison(frame, workspace[0], app)
        }
        DeepLearnPage::Sources => {
            render_sources(frame, workspace[0], app)
        }
        DeepLearnPage::Trace => {
            render_deep_learn_trace(frame, workspace[0], app)
        }
    }

    // All AI Agent tabs share one card preview surface.
    render_card_preview_panel(frame, workspace[1], app);

    let actions = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(root[4]);

    render_button(
        frame,
        actions[0],
        " AI OPTIMIZATION ",
        UiAction::DeepAgentRun,
        theme::magenta(),
        app,
    );
    render_button(
        frame,
        actions[1],
        " RUN RESEARCH ",
        UiAction::DeepLearnRunResearch,
        theme::purple(),
        app,
    );
    render_button(
        frame,
        actions[2],
        " SIMULATE GAMES ",
        UiAction::DeepLearnRunSimulation,
        theme::green(),
        app,
    );
    render_button(
        frame,
        actions[3],
        " COPY DECKLIST ",
        UiAction::DeepLearnCopyDeck,
        theme::cyan(),
        app,
    );
    render_button(
        frame,
        actions[4],
        " AI UPGRADE ",
        UiAction::DeepLearnUpgrade,
        theme::yellow(),
        app,
    );

    frame.render_widget(
        Paragraph::new(
            match app.deep_learn_page {
                DeepLearnPage::Agent => {
                    "Click GOAL • Enter run • wheel scroll"
                }
                DeepLearnPage::Chat => {
                    "Enter apply/send • Shift+Enter newline"
                }
                DeepLearnPage::Compare => {
                    "Wheel / PgUp / PgDn scroll • hover card names"
                }
                _ => {
                    "H/L tabs • wheel scroll • Esc analysis"
                }
            },
        )
        .style(Style::default().fg(theme::cyan())),
        root[5],
    );
}

fn render_deep_learn_tabs(frame: &mut Frame, area: Rect, app: &mut App) {
    let cells = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            vec![
                Constraint::Ratio(1, DeepLearnPage::ALL.len() as u32);
                DeepLearnPage::ALL.len()
            ],
        )
        .split(area);

    for (index, page) in DeepLearnPage::ALL.iter().enumerate() {
        let rect = cells[index];
        app.regions.deep_learn_tabs.push((*page, rect));

        frame.render_widget(
            Paragraph::new(
                if *page == app.deep_learn_page {
                    format!("› {} ‹", page.title())
                } else {
                    page.title().to_string()
                },
            )
            .alignment(Alignment::Center)
            .style(if *page == app.deep_learn_page {
                theme::selected()
            } else {
                Style::default().fg(theme::muted_color())
            }),
            rect,
        );
    }
}

fn render_deep_agent_chat(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(4),
        ])
        .split(area);

    let available_lines =
        rows[0].height.saturating_sub(2) as usize;

    let mut all_lines = Vec::<Line>::new();

    for message in &app.agent_chat_messages {
        let role = match message.role.as_str() {
            "user" => "YOU",
            "assistant" => "AGENT",
            _ => "SYSTEM",
        };

        let style = match message.role.as_str() {
            "user" => Style::default()
                .fg(theme::cyan())
                .add_modifier(Modifier::BOLD),
            "assistant" => Style::default().fg(theme::green()),
            _ => Style::default().fg(theme::yellow()),
        };

        let mut message_lines = message
            .content
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>();

        if message_lines.is_empty() {
            message_lines.push(String::new());
        }

        let first = message_lines.remove(0);

        all_lines.push(
            Line::from(format!("{role} › {first}"))
                .style(style),
        );

        all_lines.extend(
            message_lines.into_iter().map(|line| {
                Line::from(format!("      {line}"))
                    .style(style)
            }),
        );

        all_lines.push(Line::from(""));
    }

    let max_start =
        all_lines.len().saturating_sub(available_lines);

    let start_index = max_start
        .saturating_sub(app.agent_chat_scroll)
        .min(max_start);

    let mut rendered = all_lines
        .into_iter()
        .skip(start_index)
        .take(available_lines)
        .collect::<Vec<_>>();

    if rendered.is_empty() {
        rendered = vec![
            Line::from(
                "The Agent replies here — ask questions, refine the game plan, request cuts/adds. Say \"run optimization\" when ready to apply.",
            )
            .style(
                Style::default()
                    .fg(theme::cyan())
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            Line::from(
                "Try: \"I want this to win through combat, not combo\" • \"what should I cut for more ramp?\" • \"show decklist\" • \"run optimization with 10 candidates\".",
            ),
            Line::from(""),
            Line::from(
                "Heavy work only runs on explicit commands: run optimization / run simulation / run research.",
            ),
        ];
    }

    frame.render_widget(
        Paragraph::new(rendered)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(
                        " DECK AGENT CONVERSATION • COLLABORATIVE CHAT ",
                    )
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::purple()),
                    )
                    .style(theme::panel()),
            ),
        rows[0],
    );

    let chat_input_line =
        app.agent_chat_input.lines().count().max(1);

    let chat_input_col = app.agent_chat_input
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .chars()
        .count()
        + 1;

    let chat_input_visible =
        rows[1].height.saturating_sub(2);

    let chat_input_scroll = (chat_input_line as u16)
        .saturating_sub(chat_input_visible.max(1));

    frame.render_widget(
        Paragraph::new(format!(
            "{}▌",
            app.agent_chat_input,
        ))
            .wrap(Wrap { trim: false })
            .scroll((chat_input_scroll, 0))
            .block(
                Block::default()
                    .title(format!(
                        " MESSAGE • LINE {} COL {} • {} CHARS • ENTER SEND • SHIFT+ENTER NEWLINE ",
                        chat_input_line,
                        chat_input_col,
                        app.agent_chat_input.chars().count(),
                    ))
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::cyan()),
                    )
                    .style(theme::panel()),
            ),
        rows[1],
    );

    let buttons = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(rows[2]);

    render_button(
        frame,
        buttons[0],
        if app.agent_chat_status.contains("thinking") {
            " AGENT THINKING... "
        } else {
            " SEND MESSAGE "
        },
        UiAction::DeepAgentChatSend,
        theme::green(),
        app,
    );

    render_button(
        frame,
        buttons[1],
        " CLEAR CONVERSATION ",
        UiAction::DeepAgentChatClear,
        theme::red(),
        app,
    );

    let progress_bar = agent_ascii_progress(
        app.deep_agent_progress,
        rows[3].width.saturating_sub(20) as usize,
    );

    frame.render_widget(
        Paragraph::new(format!(
            "{}\n{}",
            if app.is_ai_optimization_running() {
                format!(
                    "{} • {}",
                    app.deep_agent_current_task,
                    app.deep_learn_status,
                )
            } else {
                app.agent_chat_status.clone()
            },
            progress_bar,
        ))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .style(
            Style::default()
                .fg(
                    if app.is_ai_optimization_running() {
                        theme::cyan()
                    } else {
                        theme::muted_color()
                    },
                ),
        )
        .block(
            Block::default()
                .title(
                    " AGENT STATUS • CHAT TO PLAN • CTRL+R RUN OPTIMIZATION ",
                )
                .borders(Borders::TOP)
                .border_style(
                    Style::default().fg(theme::muted_color()),
                ),
        ),
        rows[3],
    );
}

fn render_deep_agent(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let prompt_width =
        area.width.saturating_sub(5).max(12) as usize;

    let input_visual_lines = app
        .deep_agent_style_brief
        .split('\n')
        .map(|line| {
            let chars = line.chars().count();
            ((chars.max(1) + prompt_width - 1) / prompt_width)
                .max(1)
        })
        .sum::<usize>()
        .max(1);

    let activity_visual_lines = app
        .deep_agent_completed_tasks
        .len()
        .saturating_add(
            if app.is_ai_optimization_running() {
                1
            } else {
                0
            },
        )
        .max(1);

    let max_prompt_height =
        (area.height / 2).clamp(5, 14);

    let prompt_height = if app.deep_agent_prompt_editing {
        (input_visual_lines as u16)
            .saturating_add(2)
            .clamp(5, max_prompt_height)
    } else if app.is_ai_optimization_running()
        || !app.deep_agent_completed_tasks.is_empty()
    {
        (activity_visual_lines as u16)
            .saturating_add(2)
            .clamp(5, max_prompt_height)
    } else {
        7.min(max_prompt_height)
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),
            Constraint::Length(prompt_height),
            Constraint::Length(5),
        ])
        .split(area);

    let mut card_hit_lines =
        Vec::<(usize, String, Vec<String>)>::new();

    let body = if let Some(result) =
        app.deep_agent_result.clone()
    {
        let mut lines = vec![
            Line::from(format!(
                "TARGET B{} • {} ROUND(S)",
                result.target_bracket,
                result.rounds.len(),
            ))
            .style(
                Style::default()
                    .fg(theme::cyan())
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        match result.goal {
            OptimizationGoal::FastWin => {
                lines.push(Line::from(format!(
                    "WIN: earliest {} • likely {} • T4 {:.2}% • T5 {:.2}% • T6 {:.2}%",
                    result
                        .final_testing
                        .goldfish
                        .earliest_combo_turn
                        .map(|turn| format!("T{turn}"))
                        .unwrap_or_else(|| "none by T8".to_string()),
                    result
                        .final_testing
                        .goldfish
                        .most_likely_combo_turn
                        .map(|turn| format!("T{turn}"))
                        .unwrap_or_else(|| "none".to_string()),
                    result.final_testing.goldfish.combo_by_t4_pct,
                    result.final_testing.goldfish.combo_by_t5_pct,
                    result.final_testing.goldfish.combo_by_t6_pct,
                )));
            }

            OptimizationGoal::StaxLock => {
                if let Some(stax) = result.final_stax.as_ref() {
                    let by_turn = |turn: u8| {
                        stax.lock_by_turn_pct
                            .iter()
                            .find(|(candidate, _)| *candidate == turn)
                            .map(|(_, pct)| *pct)
                            .unwrap_or(0.0)
                    };

                    lines.push(Line::from(format!(
                        "LOCK: earliest {} • likely {} • T3 {:.2}% • T4 {:.2}% • T5 {:.2}%",
                        stax.earliest_lock_turn
                            .map(|turn| format!("T{turn}"))
                            .unwrap_or_else(|| "none by T8".to_string()),
                        stax.most_likely_lock_turn
                            .map(|turn| format!("T{turn}"))
                            .unwrap_or_else(|| "none".to_string()),
                        by_turn(3),
                        by_turn(4),
                        by_turn(5),
                    )));
                }
            }
        }

        if !result.ai_profile.gameplan.is_empty() {
            lines.push(Line::from(""));
            lines.push(
                Line::from(
                    result.ai_profile
                        .gameplan
                        .iter()
                        .take(4)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" → "),
                )
                .style(Style::default().fg(theme::magenta())),
            );
        }

        if !result.ai_profile.win_packages.is_empty() {
            lines.push(Line::from(""));

            for package in
                result.ai_profile.win_packages.iter().take(3)
            {
                let prefix =
                    format!("{}: ", package.name);
                let names = package.cards.clone();
                let line_index = lines.len();

                lines.push(
                    agent_card_text_line(
                        &prefix,
                        &names,
                        app,
                    ),
                );

                card_hit_lines.push((
                    line_index,
                    prefix,
                    names,
                ));
            }
        }

        lines.push(Line::from(""));
        lines.push(
            Line::from("ROUNDS")
                .style(
                    Style::default()
                        .fg(theme::yellow())
                        .add_modifier(Modifier::BOLD),
                ),
        );

        for round in &result.rounds {
            lines.push(
                Line::from(format!(
                    "R{} • {} • B{} • {:.2}",
                    round.round,
                    round.summary,
                    round.detected_bracket,
                    round.objective_score,
                ))
                .style(
                    Style::default().fg(
                        if round.accepted {
                            theme::green()
                        } else {
                            theme::muted_color()
                        },
                    ),
                ),
            );

            if !round.added_cards.is_empty() {
                let names = round
                    .added_cards
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>();
                let prefix = "+ ".to_string();
                let line_index = lines.len();

                lines.push(
                    agent_card_text_line(
                        &prefix,
                        &names,
                        app,
                    ),
                );

                card_hit_lines.push((
                    line_index,
                    prefix,
                    names,
                ));
            }

            if !round.removed_cards.is_empty() {
                let names = round
                    .removed_cards
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>();
                let prefix = "- ".to_string();
                let line_index = lines.len();

                lines.push(
                    agent_card_text_line(
                        &prefix,
                        &names,
                        app,
                    ),
                );

                card_hit_lines.push((
                    line_index,
                    prefix,
                    names,
                ));
            }

        }

        lines
    } else {
        let mut lines = Vec::new();

        if app.is_ai_optimization_running() {
            lines.push(
                Line::from(
                    app.deep_agent_current_task.clone(),
                )
                .style(
                    Style::default()
                        .fg(theme::cyan())
                        .add_modifier(Modifier::BOLD),
                ),
            );
        }

        for task in app
            .deep_agent_completed_tasks
            .iter()
            .rev()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            lines.push(
                Line::from(task)
                    .style(Style::default().fg(theme::green())),
            );
        }

        lines
    };

    let body_height =
        rows[0].height.saturating_sub(2) as usize;
    let max_body_scroll =
        body.len().saturating_sub(body_height.max(1));
    let body_scroll = (app.agent_page_scroll as usize)
        .min(max_body_scroll);

    app.agent_page_scroll = body_scroll as u16;

    for (line_index, prefix, names) in &card_hit_lines {
        if *line_index >= body_scroll
            && *line_index
                < body_scroll.saturating_add(body_height)
        {
            let visible_row = rows[0]
                .y
                .saturating_add(1)
                .saturating_add(
                    line_index.saturating_sub(body_scroll)
                        as u16,
                );

            register_exact_card_hits(
                app,
                rows[0],
                visible_row,
                prefix,
                names,
            );
        }
    }

    frame.render_widget(
        Paragraph::new(body)
            .scroll((app.agent_page_scroll, 0))
            .block(
                Block::default()
                    .title(" AI OPTIMIZATION ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::purple()),
                    )
                    .style(theme::panel()),
            ),
        rows[0],
    );

    app.regions.agent_prompt_box = Some(rows[1]);

    let prompt_inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(rows[1]);

    let visible_lines =
        prompt_inner[0].height.saturating_sub(2) as usize;
    let wrap_width =
        prompt_inner[0].width.saturating_sub(2).max(1)
            as usize;

    let wrap_lines = |text: &str| -> Vec<String> {
        let mut visual = Vec::<String>::new();

        for raw in text.split('\n') {
            let chars = raw.chars().collect::<Vec<_>>();

            if chars.is_empty() {
                visual.push(String::new());
                continue;
            }

            for chunk in chars.chunks(wrap_width) {
                visual.push(chunk.iter().collect());
            }
        }

        if visual.is_empty() {
            visual.push(String::new());
        }

        visual
    };

    let (source_lines, box_title, show_cursor) =
        if app.deep_agent_prompt_editing {
            let content = if app.deep_agent_style_brief.is_empty() {
                String::new()
            } else {
                app.deep_agent_style_brief.clone()
            };

            (
                wrap_lines(&content),
                " GOAL • ENTER RUN ",
                true,
            )
        } else if app.is_ai_optimization_running()
            || !app.deep_agent_completed_tasks.is_empty()
        {
            let mut lines =
                app.deep_agent_completed_tasks.clone();

            if app.is_ai_optimization_running() {
                lines.push(format!(
                    "WORKING • {}",
                    app.deep_learn_status,
                ));
            } else if app.deep_agent_result.is_some() {
                lines.push(
                    "DONE • AI Optimization complete"
                        .to_string(),
                );
            }

            (
                lines,
                " AI ACTIVITY ",
                false,
            )
        } else {
            (
                wrap_lines(
                    "Describe how you want this deck to play.\nExample: Keep the commander central, aim for Bracket 4, preserve the graveyard package, avoid stax, and prioritize the fastest reliable win.",
                ),
                " GOAL • CLICK TO EDIT ",
                false,
            )
        };

    let max_scroll =
        source_lines.len().saturating_sub(visible_lines.max(1));

    let prompt_scroll =
        if app.deep_agent_prompt_scroll == usize::MAX {
            max_scroll
        } else {
            app.deep_agent_prompt_scroll.min(max_scroll)
        };

    app.deep_agent_prompt_scroll = prompt_scroll;

    let mut shown_prompt = source_lines
        .iter()
        .skip(prompt_scroll)
        .take(visible_lines.max(1))
        .cloned()
        .collect::<Vec<_>>();

    if show_cursor {
        if shown_prompt.is_empty() {
            shown_prompt.push("▌".to_string());
        } else if let Some(last) = shown_prompt.last_mut() {
            last.push('▌');
        }
    }

    let prompt_style =
        if app.deep_agent_prompt_editing {
            Style::default().fg(theme::cyan())
        } else if app.is_ai_optimization_running() {
            Style::default().fg(theme::green())
        } else {
            Style::default().fg(theme::text())
        };

    frame.render_widget(
        Paragraph::new(
            shown_prompt
                .into_iter()
                .map(Line::from)
                .collect::<Vec<_>>(),
        )
        .wrap(Wrap { trim: false })
        .style(prompt_style)
        .block(
            Block::default()
                .title(box_title)
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(
                    Style::default().fg(
                        if app.deep_agent_prompt_editing {
                            theme::cyan()
                        } else if app.is_ai_optimization_running() {
                            theme::green()
                        } else {
                            theme::muted_color()
                        },
                    ),
                )
                .style(theme::panel()),
        ),
        prompt_inner[0],
    );

    let bar_height = prompt_inner[1].height as usize;
    let thumb = if source_lines.len() <= visible_lines.max(1) {
        0
    } else {
        ((prompt_scroll as f64
            / max_scroll.max(1) as f64)
            * bar_height.saturating_sub(1) as f64)
            .round() as usize
    };

    let scroll_bar = (0..bar_height)
        .map(|index| {
            if source_lines.len() <= visible_lines.max(1) {
                " "
            } else if index == thumb {
                "█"
            } else {
                "│"
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    frame.render_widget(
        Paragraph::new(scroll_bar)
            .style(Style::default().fg(theme::muted_color())),
        prompt_inner[1],
    );

    let footer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
        ])
        .split(rows[2]);

    if app.deep_agent_result.is_some()
        && !app.is_ai_optimization_running()
    {
        render_button(
            frame,
            footer[0],
            " SAVE BEST DECK ",
            UiAction::DeepAgentSaveBest,
            theme::green(),
            app,
        );
    }

    frame.render_widget(
        Paragraph::new(
            agent_ascii_progress(
                app.deep_agent_progress,
                footer[1].width.saturating_sub(18) as usize,
            ),
        )
        .alignment(Alignment::Center)
        .style(
            Style::default().fg(
                if app.is_ai_optimization_running() {
                    theme::cyan()
                } else {
                    theme::muted_color()
                },
            ),
        )
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(
                    Style::default().fg(theme::muted_color()),
                ),
        ),
        footer[1],
    );
}

fn selected_card_name(app: &App, name: &str) -> bool {
    app.selected_card_info
        .as_ref()
        .map(|card| card.name.eq_ignore_ascii_case(name))
        .unwrap_or(false)
}

fn agent_card_text_line(
    prefix: &str,
    names: &[String],
    app: &App,
) -> Line<'static> {
    let mut spans = Vec::new();

    if !prefix.is_empty() {
        spans.push(
            ratatui::text::Span::styled(
                prefix.to_string(),
                Style::default().fg(theme::muted_color()),
            ),
        );
    }

    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            spans.push(
                ratatui::text::Span::raw(", "),
            );
        }

        spans.push(
            ratatui::text::Span::styled(
                name.clone(),
                if selected_card_name(app, name) {
                    theme::selected()
                } else {
                    Style::default().fg(theme::text())
                },
            ),
        );
    }

    Line::from(spans)
}

fn register_exact_card_hits(
    app: &mut App,
    area: Rect,
    visible_y: u16,
    prefix: &str,
    names: &[String],
) {
    if visible_y <= area.y
        || visible_y
            >= area.y.saturating_add(area.height.saturating_sub(1))
    {
        return;
    }

    let inner_x = area.x.saturating_add(1);
    let inner_right =
        area.x.saturating_add(area.width.saturating_sub(1));

    let mut x = inner_x
        .saturating_add(prefix.chars().count() as u16);

    for (index, name) in names.iter().enumerate() {
        if index > 0 {
            x = x.saturating_add(2);
        }

        if x >= inner_right {
            break;
        }

        let width = (name.chars().count() as u16)
            .min(inner_right.saturating_sub(x));

        if width == 0 {
            break;
        }

        app.regions.named_cards.push((
            name.clone(),
            Rect::new(x, visible_y, width, 1),
        ));

        x = x.saturating_add(width);
    }
}

fn agent_ascii_progress(
    percent: u8,
    requested_width: usize,
) -> String {
    let width = requested_width.clamp(12, 60);
    let pct = percent.min(100);
    let filled =
        ((width as u16 * pct as u16) / 100) as usize;

    format!(
        "[{}{}] {:>3}%",
        "#".repeat(filled),
        "-".repeat(width.saturating_sub(filled)),
        pct,
    )
}


fn register_card_names_in_row(
    app: &mut App,
    area: Rect,
    y: u16,
    names: &[String],
) {
    if names.is_empty()
        || y < area.y
        || y >= area.y.saturating_add(area.height)
    {
        return;
    }

    let count = names.len().min(8) as u16;
    let width = area.width.saturating_sub(2).max(count);
    let each = (width / count.max(1)).max(1);

    for (index, name) in names.iter().take(8).enumerate() {
        app.regions.named_cards.push((
            name.clone(),
            Rect::new(
                area.x + 1 + each.saturating_mul(index as u16),
                y,
                each,
                1,
            ),
        ));
    }
}

fn render_deep_learn_overview(frame: &mut Frame, area: Rect, app: &mut App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
        ])
        .split(area);

    let model = if let Some(report) = app.deep_learn.as_ref() {
        format!(
            "MODEL READY\n\nConfidence: {}%\nEDHREC signals: {}\nReference decks: {}\nRecurring reference combos: {}\nCore coverage: {:.0}%\n\nNormal themes:\n{}\n\nLearned role targets:\n{}", 
            report.confidence,
            report.edhrec_signals,
            report.reference_decks,
            report.reference_combos.len(),
            report.core_coverage_pct,
            report.normal_themes.iter().take(8).cloned().collect::<Vec<_>>().join(" / "),
            report.role_targets,
        )
    } else {
        "Research has not been run yet.\n\nRun RESEARCH to learn the commander's normal shell, public reference patterns, card support, confidence, and structural role targets."
            .to_string()
    };

    frame.render_widget(
        Paragraph::new(model)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" MODEL OVERVIEW ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::purple()))
                    .style(theme::panel()),
            ),
        cols[0],
    );

    let testing = if let Some(test) = app.deep_learn_testing.as_ref() {
        format!(
            "LATEST GAME TEST\n\n{} intensity\n{} games\nEarliest: {}\nMost likely: {}\nMedian winning: {}\nWon by T4: {:.1}%\nWon by T5: {:.1}%\nWon by T6: {:.1}%\n\nOpen WIN SPEED for the full match table.",
            test.intensity.label(),
            test.goldfish.runs,
            test.goldfish.earliest_combo_turn.map(|t| format!("T{t}")).unwrap_or_else(|| "none by T8".to_string()),
            test.goldfish.most_likely_combo_turn.map(|t| format!("T{t}")).unwrap_or_else(|| "-".to_string()),
            test.goldfish.median_combo_turn.map(|t| format!("T{t}")).unwrap_or_else(|| "-".to_string()),
            test.goldfish.combo_by_t4_pct,
            test.goldfish.combo_by_t5_pct,
            test.goldfish.combo_by_t6_pct,
        )
    } else {
        format!(
            "NO GAME TEST YET\n\nSelected: {}\n{} draw simulations\n{} goldfish games\n\nUse SIMULATE when you want Muldrotha-A-style speed testing.",
            app.deep_learn_intensity.label(),
            app.deep_learn_intensity.draw_runs(),
            app.deep_learn_intensity.goldfish_runs(),
        )
    };

    frame.render_widget(
        Paragraph::new(testing)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" TESTING OVERVIEW ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::cyan()))
                    .style(theme::panel()),
            ),
        cols[1],
    );
}

fn render_deep_learn_research(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let Some(report) = app.deep_learn.clone() else {
        let text = if app.deep_learn_trace.is_empty() {
            "Run research to build the commander model.\n\nThe reference pass now gathers dozens of matching public lists where available, measures card consensus and recurring card packages, and checks complete Commander Spellbook combos found in those lists."
                .to_string()
        } else {
            app.deep_learn_trace.join("\n")
        };

        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .title(" RESEARCH MODEL ")
                        .borders(Borders::ALL)
                    .border_type(theme::border_type())
                        .border_style(Style::default().fg(theme::purple()))
                        .style(theme::panel()),
                ),
            area,
        );
        return;
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(report.findings.join("\n\n"))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(format!(
                        " MODEL • {} REFERENCE DECKS ",
                        report.reference_decks
                    ))
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::purple()))
                    .style(theme::panel()),
            ),
        cols[0],
    );

    let mut consensus = Vec::<Line>::new();

    consensus.push(
        Line::from("COMMON CARDS")
            .style(
                Style::default()
                    .fg(theme::cyan())
                    .add_modifier(Modifier::BOLD),
            ),
    );

    for (index, line) in report
        .reference_core_cards
        .iter()
        .take(14)
        .enumerate()
    {
        consensus.push(Line::from(format!("• {line}")));

        if let Some(name) = line.split(" — ").next() {
            app.regions.named_cards.push((
                name.trim().to_string(),
                Rect::new(
                    cols[1].x + 1,
                    cols[1].y + 2 + index as u16,
                    cols[1].width.saturating_sub(2),
                    1,
                ),
            ));
        }
    }

    consensus.push(Line::from(""));
    consensus.push(
        Line::from("RECURRING PACKAGES")
            .style(
                Style::default()
                    .fg(theme::yellow())
                    .add_modifier(Modifier::BOLD),
            ),
    );

    let package_start_y =
        cols[1].y + 4 + report.reference_core_cards.len().min(14) as u16;

    for (index, line) in report
        .reference_packages
        .iter()
        .take(10)
        .enumerate()
    {
        consensus.push(Line::from(format!("• {line}")));

        if let Some(cards) = line.split(" — ").next() {
            let names = cards
                .split(" + ")
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
                .collect::<Vec<_>>();

            register_card_names_in_row(
                app,
                cols[1],
                package_start_y + index as u16,
                &names,
            );
        }
    }

    frame.render_widget(
        Paragraph::new(consensus)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" REFERENCE CONSENSUS ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::cyan()))
                    .style(theme::panel()),
            ),
        cols[1],
    );

    let combo_text = if report.reference_combos.is_empty() {
        "No recurring complete Commander Spellbook combos were found in the sampled reference lists."
            .to_string()
    } else {
        for (index, line) in report
            .reference_combos
            .iter()
            .take(14)
            .enumerate()
        {
            if let Some(cards) = line.split(" — ").next() {
                let names = cards
                    .split(" + ")
                    .map(|name| name.trim().to_string())
                    .filter(|name| !name.is_empty())
                    .collect::<Vec<_>>();

                register_card_names_in_row(
                    app,
                    cols[2],
                    cols[2].y + 1 + (index as u16 * 2),
                    &names,
                );
            }
        }

        report
            .reference_combos
            .iter()
            .take(14)
            .map(|line| format!("• {line}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    frame.render_widget(
        Paragraph::new(combo_text)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" COMBOS / AI WIN PACKAGES ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::magenta()))
                    .style(theme::panel()),
            ),
        cols[2],
    );
}

fn render_deep_learn_simulate(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    let body = if let Some(meta) = app.meta_matchup.as_ref() {
        let mut text = vec![
            format!(
                "META MATCHUP RESULT • {} POD GAMES • {} META DECKS",
                meta.runs,
                meta.opponents_used,
            ),
            String::new(),
            format!(
                "Outright win rate:  {:.2}%",
                meta.outright_win_pct,
            ),
            format!(
                "Equity win rate:    {:.2}%",
                meta.equity_win_pct,
            ),
            format!(
                "Shared first:       {:.2}%",
                meta.shared_first_pct,
            ),
            format!(
                "Loss rate:          {:.2}%",
                meta.loss_pct,
            ),
            format!(
                "Average winning turn: {}",
                meta.average_winning_turn
                    .map(|turn| format!("{turn:.2}"))
                    .unwrap_or_else(|| "n/a".to_string()),
            ),
            String::new(),
            "MATCHUPS".to_string(),
        ];

        for opponent in meta.opponents.iter().take(12) {
            text.push(format!(
                "{:.1}% H2H • {} • {}",
                opponent.head_to_head_win_pct,
                opponent.source,
                opponent.title,
            ));
        }

        text.push(String::new());
        text.push(meta.caveat.clone());
        text.join("\n")
    } else if let Some(test) = app.deep_learn_testing.as_ref() {
        format!(
            "LATEST SOLO TEST\n\n{} games tested\nKeepable opening: {:.1}%\n3 mana by T3: {:.1}%\n4 mana by T4: {:.1}%\n5 mana by T5: {:.1}%\nCombo by T4: {:.1}%\nCombo by T5: {:.1}%\nCombo by T6: {:.1}%\n\n{}",
            test.goldfish.runs,
            test.goldfish.keepable_pct,
            test.goldfish.three_mana_t3_pct,
            test.goldfish.four_mana_t4_pct,
            test.goldfish.five_mana_t5_pct,
            test.goldfish.combo_by_t4_pct,
            test.goldfish.combo_by_t5_pct,
            test.goldfish.combo_by_t6_pct,
            test.goldfish.caveat,
        )
    } else {
        "No test result yet.\n\nDefault testing is Thorough. In AI Agent Chat you can request an exact workload, for example:\n\nsimulate 1,000 games\nsimulate 100,000 games\nrun 1,000 games against meta decks and report win rate"
            .to_string()
    };

    frame.render_widget(
        Paragraph::new(body)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" GAME TESTING ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::cyan()),
                    )
                    .style(theme::panel()),
            ),
        rows[0],
    );

    render_button(
        frame,
        rows[1],
        " RUN THOROUGH TEST ",
        UiAction::DeepLearnRunSimulation,
        theme::green(),
        app,
    );
}

fn render_deep_learn_win_speed(frame: &mut Frame, area: Rect, app: &mut App) {
    if let Some(agent) = app.deep_agent_result.clone() {
        if agent.goal == OptimizationGoal::StaxLock {
            if let Some(stax) = agent.final_stax {
                render_deep_stax_speed(frame, area, app, &stax);
                return;
            }
        }
    }

    let Some(test) = app.deep_learn_testing.clone() else {
        frame.render_widget(
            Paragraph::new("No game test yet.\n\nRun a Thorough test or request an exact number of games in AI Agent Chat.")
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .title(" WIN SPEED ")
                        .borders(Borders::ALL)
                    .border_type(theme::border_type())
                        .border_style(Style::default().fg(theme::green()))
                        .style(theme::panel()),
                ),
            area,
        );
        return;
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(52),
            Constraint::Percentage(48),
        ])
        .split(area);

    let runs = test.goldfish.runs.max(1) as f64;
    let mut lines = vec![
        Line::from(format!("{} GAMES TESTED", test.goldfish.runs))
            .style(Style::default().fg(theme::cyan()).add_modifier(Modifier::BOLD)),
        Line::from(""),
        Line::from(format!("{:<12} {:>12} {:>10}", "RESULT", "MATCHES", "RATE"))
            .style(Style::default().fg(theme::yellow())),
    ];

    for (turn, count) in &test.goldfish.combo_turn_counts {
        lines.push(Line::from(format!(
            "T{:<11} {:>12} {:>9.2}%",
            turn,
            count,
            *count as f64 / runs * 100.0,
        )));
    }

    lines.push(Line::from(format!(
        "{:<12} {:>12} {:>9.2}%",
        "NO WIN T8",
        test.goldfish.no_combo_by_t8_count,
        test.goldfish.no_combo_by_t8_count as f64 / runs * 100.0,
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(format!(
        "Earliest observed: {}",
        test.goldfish.earliest_combo_turn.map(|t| format!("T{t}")).unwrap_or_else(|| "none".to_string())
    )));
    lines.push(Line::from(format!(
        "Most likely:       {}",
        test.goldfish.most_likely_combo_turn.map(|t| format!("T{t}")).unwrap_or_else(|| "none".to_string())
    )));
    lines.push(Line::from(format!(
        "Median winning:    {}",
        test.goldfish.median_combo_turn.map(|t| format!("T{t}")).unwrap_or_else(|| "none".to_string())
    )));

    for turn in [3u8, 4, 5, 6, 7, 8] {
        if let Some((_, pct)) = test.goldfish.combo_by_turn_pct.iter().find(|(t, _)| *t == turn) {
            lines.push(Line::from(format!("Chance won by T{turn}: {:>6.2}%", pct)));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" MATCH BREAKDOWN ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::green()))
                    .style(theme::panel()),
            ),
        cols[0],
    );

    let mut wins = vec![
        Line::from("KNOWN WIN LINES")
            .style(Style::default().fg(theme::yellow()).add_modifier(Modifier::BOLD)),
        Line::from(""),
    ];

    if test.goldfish.combo_line_hits.is_empty() {
        wins.push(Line::from("No known Spellbook line assembled by T8."));
    } else {
        for (index, (cards, count)) in
            test.goldfish.combo_line_hits.iter().enumerate()
        {
            register_card_names_in_row(
                app,
                cols[1],
                cols[1].y + 3 + (index as u16 * 2),
                cards,
            );

            wins.push(Line::from(cards.join(" + ")));
            wins.push(
                Line::from(format!(
                    "  {} wins • {:.2}% of all games",
                    count,
                    *count as f64 / runs * 100.0,
                ))
                .style(Style::default().fg(theme::muted_color())),
            );
        }
    }

    wins.push(Line::from(""));
    wins.push(
        Line::from("Goldfish/known-line estimate; not a four-player opponent model.")
            .style(Style::default().fg(theme::muted_color())),
    );

    frame.render_widget(
        Paragraph::new(wins)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" HOW THE DECK WON ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::purple()))
                    .style(theme::panel()),
            ),
        cols[1],
    );
}

fn render_deep_stax_speed(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
    stax: &crate::deep_agent::StaxSimulation,
) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(52),
            Constraint::Percentage(48),
        ])
        .split(area);

    let runs = stax.runs.max(1) as f64;

    let mut breakdown = vec![
        Line::from(format!(
            "{} LOCK TESTS • {} recognized stax piece(s)",
            stax.runs,
            stax.stax_cards.len(),
        ))
            .style(
                Style::default()
                    .fg(theme::cyan())
                    .add_modifier(Modifier::BOLD),
            ),
        Line::from(""),
        Line::from(format!(
            "{:<12} {:>12} {:>10}",
            "RESULT", "MATCHES", "RATE"
        ))
        .style(Style::default().fg(theme::yellow())),
    ];

    for (turn, count) in &stax.lock_turn_counts {
        breakdown.push(Line::from(format!(
            "LOCK T{:<6} {:>12} {:>9.2}%",
            turn,
            count,
            *count as f64 / runs * 100.0,
        )));
    }

    breakdown.push(Line::from(format!(
        "{:<12} {:>12} {:>9.2}%",
        "NO LOCK T8",
        stax.no_lock_by_t8_count,
        stax.no_lock_by_t8_count as f64 / runs * 100.0,
    )));

    breakdown.push(Line::from(""));
    breakdown.push(Line::from(format!(
        "Earliest lock: {}",
        stax.earliest_lock_turn
            .map(|turn| format!("T{turn}"))
            .unwrap_or_else(|| "none".to_string()),
    )));
    breakdown.push(Line::from(format!(
        "Most likely:   {}",
        stax.most_likely_lock_turn
            .map(|turn| format!("T{turn}"))
            .unwrap_or_else(|| "none".to_string()),
    )));
    breakdown.push(Line::from(format!(
        "Median lock:   {}",
        stax.median_lock_turn
            .map(|turn| format!("T{turn}"))
            .unwrap_or_else(|| "none".to_string()),
    )));

    for turn in [2u8, 3, 4, 5, 6, 7, 8] {
        if let Some((_, pct)) =
            stax.lock_by_turn_pct.iter().find(|(t, _)| *t == turn)
        {
            breakdown.push(Line::from(format!(
                "Chance locked by T{turn}: {:>6.2}%",
                pct,
            )));
        }
    }

    frame.render_widget(
        Paragraph::new(breakdown)
            .block(
                Block::default()
                    .title(" STAX LOCK BREAKDOWN ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::magenta()))
                    .style(theme::panel()),
            ),
        cols[0],
    );

    let mut pieces = vec![
        Line::from("LOCK PIECES SEEN")
            .style(
                Style::default()
                    .fg(theme::yellow())
                    .add_modifier(Modifier::BOLD),
            ),
        Line::from(""),
    ];

    for (index, (name, hits)) in
        stax.lock_piece_hits.iter().take(18).enumerate()
    {
        pieces.push(Line::from(format!(
            "{} • {} cast(s) • {:.2}% of tests",
            name,
            hits,
            *hits as f64 / runs * 100.0,
        )));

        app.regions.named_cards.push((
            name.clone(),
            Rect::new(
                cols[1].x + 1,
                cols[1].y + 3 + index as u16,
                cols[1].width.saturating_sub(2),
                1,
            ),
        ));
    }

    pieces.push(Line::from(""));
    pieces.push(
        Line::from(stax.caveat.clone())
            .style(Style::default().fg(theme::muted_color())),
    );

    frame.render_widget(
        Paragraph::new(pieces)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" HOW THE DECK LOCKED ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::purple()))
                    .style(theme::panel()),
            ),
        cols[1],
    );
}

fn render_deep_learn_upgrade(frame: &mut Frame, area: Rect, app: &mut App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    let current = app.parsed.as_ref().map(|d| d.name.as_str()).unwrap_or("No deck");

    let upgrade_text =
        if let Some(agent) = app.deep_agent_result.as_ref() {
            format!(
                "AI OPTIMIZATION RESULT\n\nCurrent deck: {current}\nOptimized deck: {}\nTarget bracket: Bracket {}\nRounds: {}\nFinal games: {}\nEarliest win/line: {}\n\nOpen COMPARE for the exact cuts and additions.",
                agent.best.deck.name,
                agent.target_bracket,
                agent.rounds.len(),
                agent.final_testing.goldfish.runs,
                agent.final_testing.goldfish
                    .earliest_combo_turn
                    .map(|turn| format!("T{turn}"))
                    .unwrap_or_else(|| "none by T8".to_string()),
            )
        } else {
            format!(
                "AI-DRIVEN UPGRADE\n\nCurrent deck: {current}\nTarget bracket: Bracket {}",
                app.deep_learn_upgrade_bracket,
            )
        };

    frame.render_widget(
        Paragraph::new(upgrade_text)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" UPGRADE WORKSPACE ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::yellow()))
                .style(theme::panel()),
        ),
        rows[0],
    );

    render_button(
        frame,
        rows[1],
        " COPY CURRENT DECKLIST ",
        UiAction::DeepLearnCopyDeck,
        theme::cyan(),
        app,
    );
    render_button(
        frame,
        rows[2],
        if app.deep_agent_result.is_some() {
            " SAVE AI OPTIMIZED DECK "
        } else {
            " GENERATE AI UPGRADE / CHOOSE BRACKET "
        },
        if app.deep_agent_result.is_some() {
            UiAction::DeepAgentSaveBest
        } else {
            UiAction::DeepLearnUpgrade
        },
        if app.deep_agent_result.is_some() {
            theme::green()
        } else {
            theme::yellow()
        },
        app,
    );
}

fn render_deep_agent_comparison(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let Some(comparison) =
        app.deep_agent_comparison.clone()
    else {
        frame.render_widget(
            Paragraph::new(
                "Run AI Optimization to compare the current deck with the accepted optimized deck.",
            )
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title(" DECK COMPARISON ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::purple()),
                    )
                    .style(theme::panel()),
            ),
            area,
        );
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(8),
        ])
        .split(area);

    let summary = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(rows[0]);

    frame.render_widget(
        Paragraph::new(format!(
            "{}\n{} main-deck cards",
            comparison.original_name,
            comparison.original_main_count,
        ))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" ORIGINAL ")
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(
                    Style::default().fg(theme::muted_color()),
                )
                .style(theme::panel()),
        ),
        summary[0],
    );

    frame.render_widget(
        Paragraph::new(format!(
            "{}\n{} main-deck cards • {} unchanged",
            comparison.optimized_name,
            comparison.optimized_main_count,
            comparison.unchanged_main,
        ))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" AI OPTIMIZED ")
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(
                    Style::default().fg(theme::green()),
                )
                .style(theme::panel()),
        ),
        summary[1],
    );

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(rows[1]);

    let visible =
        cols[0].height.saturating_sub(2) as usize;

    let max_items = comparison
        .removed
        .len()
        .max(comparison.added.len());

    let max_scroll =
        max_items.saturating_sub(visible.max(1));

    let scroll =
        app.compare_scroll.min(max_scroll);

    app.compare_scroll = scroll;

    let mut cut_lines = Vec::<Line>::new();

    for (visible_offset, entry) in comparison
        .removed
        .iter()
        .skip(scroll)
        .take(visible.max(1))
        .enumerate()
    {
        let prefix = format!(
            "- {}x [{}] ",
            entry.quantity,
            entry.zone,
        );

        cut_lines.push(
            Line::from(vec![
                ratatui::text::Span::styled(
                    prefix.clone(),
                    Style::default()
                        .fg(theme::red())
                        .add_modifier(Modifier::BOLD),
                ),
                ratatui::text::Span::styled(
                    entry.name.clone(),
                    if selected_card_name(app, &entry.name) {
                        theme::selected()
                    } else {
                        Style::default().fg(theme::red())
                    },
                ),
            ]),
        );

        register_exact_card_hits(
            app,
            cols[0],
            cols[0]
                .y
                .saturating_add(1)
                .saturating_add(visible_offset as u16),
            &prefix,
            std::slice::from_ref(&entry.name),
        );
    }

    if cut_lines.is_empty() {
        cut_lines.push(
            Line::from("No cuts.")
                .style(
                    Style::default()
                        .fg(theme::muted_color()),
                ),
        );
    }

    let mut add_lines = Vec::<Line>::new();

    for (visible_offset, entry) in comparison
        .added
        .iter()
        .skip(scroll)
        .take(visible.max(1))
        .enumerate()
    {
        let prefix = format!(
            "+ {}x [{}] ",
            entry.quantity,
            entry.zone,
        );

        add_lines.push(
            Line::from(vec![
                ratatui::text::Span::styled(
                    prefix.clone(),
                    Style::default()
                        .fg(theme::green())
                        .add_modifier(Modifier::BOLD),
                ),
                ratatui::text::Span::styled(
                    entry.name.clone(),
                    if selected_card_name(app, &entry.name) {
                        theme::selected()
                    } else {
                        Style::default().fg(theme::green())
                    },
                ),
            ]),
        );

        register_exact_card_hits(
            app,
            cols[1],
            cols[1]
                .y
                .saturating_add(1)
                .saturating_add(visible_offset as u16),
            &prefix,
            std::slice::from_ref(&entry.name),
        );
    }

    if add_lines.is_empty() {
        add_lines.push(
            Line::from("No additions.")
                .style(
                    Style::default()
                        .fg(theme::muted_color()),
                ),
        );
    }

    frame.render_widget(
        Paragraph::new(cut_lines)
            .block(
                Block::default()
                    .title(format!(
                        " - CUTS • {} CHANGE(S) ",
                        comparison.removed.len(),
                    ))
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::red()),
                    )
                    .style(theme::panel()),
            ),
        cols[0],
    );

    frame.render_widget(
        Paragraph::new(add_lines)
            .block(
                Block::default()
                    .title(format!(
                        " + ADDS • {} CHANGE(S) ",
                        comparison.added.len(),
                    ))
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::green()),
                    )
                    .style(theme::panel()),
            ),
        cols[1],
    );
}

fn render_deep_learn_recommendations(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let Some(agent) = app.deep_agent_result.clone() else {
        frame.render_widget(
            Paragraph::new(
                "Run AI Optimization to populate ranked deck iterations.",
            )
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .title(" RECOMMENDED DECK ITERATIONS ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::green()),
                    )
                    .style(theme::panel()),
            ),
            area,
        );
        return;
    };

    let iterations = if agent.top_iterations.is_empty() {
        Vec::new()
    } else {
        agent.top_iterations.clone()
    };

    if iterations.is_empty() {
        frame.render_widget(
            Paragraph::new(
                "No ranked candidate iterations were cached for this run. Run AI Optimization again to create ranked deck candidates.",
            )
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" RECOMMENDED DECK ITERATIONS ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::green()),
                    )
                    .style(theme::panel()),
            ),
            area,
        );
        return;
    }

    let selected = app
        .deep_learn_selected
        .min(iterations.len().saturating_sub(1));
    app.deep_learn_selected = selected;

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(38),
            Constraint::Percentage(34),
            Constraint::Percentage(28),
        ])
        .split(area);

    let mut iteration_lines = Vec::<Line>::new();
    let visible_entries =
        (cols[0].height.saturating_sub(2) as usize / 2).max(1);
    let list_start = selected
        .saturating_add(1)
        .saturating_sub(visible_entries)
        .min(iterations.len().saturating_sub(visible_entries));

    for (rank, iteration) in iterations
        .iter()
        .enumerate()
        .skip(list_start)
        .take(visible_entries)
    {
        let result = &iteration.testing.goldfish;
        let line = match agent.goal {
            OptimizationGoal::FastWin => format!(
                "#{} • candidate {} • score {:.2}\n    earliest {} • T4 {:.1}% • T5 {:.1}% • B{}",
                rank + 1,
                iteration.round,
                iteration.objective_score,
                result
                    .earliest_combo_turn
                    .map(|turn| format!("T{turn}"))
                    .unwrap_or_else(|| "none".to_string()),
                result.combo_by_t4_pct,
                result.combo_by_t5_pct,
                iteration.detected_bracket,
            ),
            OptimizationGoal::StaxLock => {
                let stax = iteration.stax.as_ref();
                format!(
                    "#{} • candidate {} • score {:.2}\n    earliest lock {} • B{}",
                    rank + 1,
                    iteration.round,
                    iteration.objective_score,
                    stax.and_then(|value| value.earliest_lock_turn)
                        .map(|turn| format!("T{turn}"))
                        .unwrap_or_else(|| "none".to_string()),
                    iteration.detected_bracket,
                )
            }
        };

        let mut parts = line.splitn(2, '\n');
        let first = parts.next().unwrap_or_default();
        let second = parts.next().unwrap_or_default();

        iteration_lines.push(
            Line::from(first.to_string()).style(
                if rank == selected {
                    theme::selected()
                } else {
                    Style::default().fg(theme::text())
                },
            ),
        );
        iteration_lines.push(
            Line::from(second.to_string()).style(
                if rank == selected {
                    theme::selected()
                } else {
                    Style::default().fg(theme::muted_color())
                },
            ),
        );

        let visible_rank = rank.saturating_sub(list_start);
        let y = cols[0]
            .y
            .saturating_add(1)
            .saturating_add((visible_rank * 2) as u16);

        if y < cols[0].y.saturating_add(cols[0].height.saturating_sub(1)) {
            let preview_name = iteration
                .deck
                .deck
                .commanders
                .first()
                .or_else(|| iteration.deck.deck.main.first())
                .map(|entry| entry.name.clone())
                .unwrap_or_else(|| format!("Candidate {}", iteration.round));

            app.regions.deep_learn_cards.push((
                rank,
                preview_name,
                Rect::new(
                    cols[0].x + 1,
                    y,
                    cols[0].width.saturating_sub(2),
                    2.min(cols[0].height.saturating_sub(2)),
                ),
            ));
        }
    }

    frame.render_widget(
        Paragraph::new(iteration_lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(format!(
                        " TOP {} ITERATIONS • CLICK / ↑↓ ",
                        iterations.len(),
                    ))
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::green()),
                    )
                    .style(theme::panel()),
            ),
        cols[0],
    );

    let iteration = &iterations[selected];
    let cards = iteration
        .deck
        .deck
        .commanders
        .iter()
        .chain(iteration.deck.deck.main.iter())
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();

    render_named_card_list(
        frame,
        cols[1],
        &format!(
            " #{} • CANDIDATE {} • {} CARDS ",
            selected + 1,
            iteration.round,
            cards.len(),
        ),
        &cards,
        app,
    );

    render_card_preview_panel(frame, cols[2], app);
}


fn render_deep_learn_trace(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let mut raw = app.deep_learn_trace.clone();

    if !app.deep_agent_trace.is_empty() {
        raw.push("".to_string());
        raw.push("=== AI OPTIMIZATION ===".to_string());
        raw.extend(app.deep_agent_trace.clone());
    }

    if let Some(report) = app.deep_learn.as_ref() {
        raw.extend(report.decision_trace.clone());
    }

    if raw.is_empty() {
        raw.push(
            "No trace yet. Run RESEARCH, SIMULATE, or AGENT."
                .to_string(),
        );
    }

    raw.insert(
        0,
        format!(
            "Persistent application log: {}",
            crate::logging::log_path(),
        ),
    );
    raw.insert(1, String::new());

    let inner_height =
        area.height.saturating_sub(2) as usize;
    let start = app.trace_scroll as usize;
    let end = (start + inner_height).min(raw.len());
    let mut lines = Vec::<Line>::new();

    for (screen_offset, index) in (start..end).enumerate() {
        let hovered = app.hovered_text
            == Some((TextPane::Trace, index));

        lines.push(
            Line::from(raw[index].clone())
                .style(
                    if hovered {
                        theme::selected()
                    } else {
                        Style::default().fg(theme::text())
                    },
                ),
        );

        app.regions.text_rows.push((
            TextPane::Trace,
            index,
            Rect::new(
                area.x + 1,
                area.y + 1 + screen_offset as u16,
                area.width.saturating_sub(2),
                1,
            ),
        ));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" EVIDENCE / DECISION / SIMULATION TRACE • WHEEL / PGUP / PGDN • HOVER HIGHLIGHTS ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::purple()))
                    .style(theme::panel()),
            ),
        area,
    );
}

fn render_settings_screen(frame: &mut Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_banner(
        frame,
        outer[0],
        "COMMANDER LAB",
        "SETTINGS",
    );

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),
            Constraint::Min(40),
        ])
        .split(outer[1]);

    let sidebar = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(body[0]);

    render_button(
        frame,
        sidebar[0],
        if app.settings_tab == SettingsTab::Theme {
            " › THEME "
        } else {
            "   THEME "
        },
        UiAction::SettingsTabTheme,
        if app.settings_tab == SettingsTab::Theme {
            theme::cyan()
        } else {
            theme::muted_color()
        },
        app,
    );

    render_button(
        frame,
        sidebar[1],
        if app.settings_tab == SettingsTab::AiModel {
            " › AI MODEL "
        } else {
            "   AI MODEL "
        },
        UiAction::SettingsTabAiModel,
        if app.settings_tab == SettingsTab::AiModel {
            theme::magenta()
        } else {
            theme::muted_color()
        },
        app,
    );

    render_button(
        frame,
        sidebar[2],
        if app.settings_tab == SettingsTab::Pricing {
            " › PRICING "
        } else {
            "   PRICING "
        },
        UiAction::SettingsTabPricing,
        if app.settings_tab == SettingsTab::Pricing {
            theme::yellow()
        } else {
            theme::muted_color()
        },
        app,
    );

    frame.render_widget(
        Paragraph::new(
            "←/→ switch tabs\nEsc return\n\nSettings are saved locally.",
        )
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(theme::muted_color()))
        .block(
            Block::default()
                .title(" SETTINGS ")
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(
                    Style::default().fg(theme::muted_color()),
                )
                .style(theme::panel()),
        ),
        sidebar[3],
    );

    match app.settings_tab {
        SettingsTab::Theme => {
            render_theme_settings(frame, body[1], app)
        }
        SettingsTab::AiModel => {
            render_ai_model_settings(frame, body[1], app)
        }
        SettingsTab::Pricing => {
            render_pricing_settings(frame, body[1], app)
        }
    }

    frame.render_widget(
        Paragraph::new(format!(
            "{}     Esc: back",
            app.status,
        ))
        .style(
            Style::default()
                .fg(theme::muted_color())
                .bg(theme::bg()),
        ),
        outer[2],
    );
}

fn render_theme_settings(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(8),
        ])
        .split(area);

    render_button(
        frame,
        rows[0],
        &format!(
            " CORNERS                 - {} ",
            theme::edge_style().label(),
        ),
        UiAction::ThemeToggleEdges,
        theme::cyan(),
        app,
    );

    render_button(
        frame,
        rows[1],
        &format!(
            " BACKGROUND              - {} ",
            if theme::transparent_background() {
                "TRANSPARENT"
            } else {
                "OPAQUE"
            },
        ),
        UiAction::ThemeToggleTransparency,
        theme::cyan(),
        app,
    );

    render_button(
        frame,
        rows[2],
        " SAVE CURRENT THEME      - OPEN ",
        UiAction::ThemeSaveNamed,
        theme::green(),
        app,
    );

    render_theme_content(frame, rows[3], app);
}


fn render_ai_model_settings(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let config = AiConfig::load_or_create();
    let api_mode =
        config.provider == crate::ai::AiProvider::Api;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(6),
        ])
        .split(area);

    render_button(
        frame,
        rows[0],
        &format!(
            " AI PROVIDER             - {} ",
            config.provider.label(),
        ),
        UiAction::SettingsCycleAiProvider,
        theme::magenta(),
        app,
    );

    render_button(
        frame,
        rows[1],
        &format!(
            " AI MODEL                - {} ▼ ",
            config.model,
        ),
        UiAction::SettingsOpenModelPicker,
        theme::blue(),
        app,
    );

    render_button(
        frame,
        rows[2],
        &format!(
            " WEB RESEARCH            - {} ",
            if config.web_research { "ON" } else { "OFF" },
        ),
        UiAction::SettingsToggleWebResearch,
        theme::cyan(),
        app,
    );

    if api_mode {
        render_button(
            frame,
            rows[3],
            &format!(
                " API BASE URL            - {} ",
                config.base_url,
            ),
            UiAction::SettingsEditApiUrl,
            theme::yellow(),
            app,
        );

        let key_status = if config.api_key.is_empty() {
            "NOT SET".to_string()
        } else {
            let tail = config
                .api_key
                .chars()
                .rev()
                .take(4)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>();
            format!("••••••••{tail}")
        };

        render_button(
            frame,
            rows[4],
            &format!(
                " API KEY                 - {} ",
                key_status,
            ),
            UiAction::SettingsEditApiKey,
            theme::yellow(),
            app,
        );

        frame.render_widget(
            Paragraph::new(
                " API FORMAT              - OPENAI COMPATIBLE",
            )
            .style(
                Style::default()
                    .fg(theme::muted_color())
                    .bg(theme::panel_color()),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::muted_color()),
                    ),
            ),
            rows[5],
        );
    } else {
        frame.render_widget(
            Paragraph::new(format!(
                " LOCAL ENDPOINT          - {}",
                config.base_url,
            ))
            .style(
                Style::default()
                    .fg(theme::muted_color())
                    .bg(theme::panel_color()),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default()
                            .fg(theme::muted_color()),
                    ),
            ),
            rows[3],
        );
    }

    let help = match config.provider {
        crate::ai::AiProvider::LlamaCpp => {
            "llama.cpp is managed locally and remains the default. Select MODEL to open the model list. Changes apply on restart."
        }
        crate::ai::AiProvider::Ollama => {
            "Ollama is only installed/started while OLLAMA is selected. Select MODEL to choose the Ollama model."
        }
        crate::ai::AiProvider::Api => {
            "API mode uses an OpenAI-compatible /v1/chat/completions endpoint. The API key is masked in the UI and is never written to Agent logs. Configure the provider base URL and exact model name here."
        }
    };

    frame.render_widget(
        Paragraph::new(help)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" AI CONFIGURATION ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::blue()),
                    )
                    .style(theme::panel()),
            ),
        rows[6],
    );
}


fn render_pricing_settings(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(8),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(format!(
            "Display currency: {}\nUSD is the default for card and deck prices.",
            theme::price_currency().label(),
        ))
        .block(
            Block::default()
                .title(" PRICE DISPLAY ")
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(
                    Style::default().fg(theme::yellow()),
                )
                .style(theme::panel()),
        ),
        rows[0],
    );

    render_button(
        frame,
        rows[1],
        &format!(
            " CURRENCY: {} ",
            theme::price_currency().label(),
        ),
        UiAction::SettingsCycleCurrency,
        theme::yellow(),
        app,
    );

    frame.render_widget(
        Paragraph::new(
            "Cycle USD → EUR → MTGO TIX. Prices never silently fall back to another currency. Cardhoarder/MTGO controls are only shown when MTGO TIX is selected.",
        )
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" PRICING POLICY ")
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(
                    Style::default().fg(theme::muted_color()),
                )
                .style(theme::panel()),
        ),
        rows[2],
    );
}


fn render_theme_content(frame: &mut Frame, area: Rect, app: &mut App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28),
            Constraint::Percentage(36),
            Constraint::Percentage(36),
        ])
        .split(area);

    let preset_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(7),
        ])
        .split(cols[0]);

    let visible = preset_rows[0].height.saturating_sub(2) as usize;

    let items = app
        .theme_library
        .iter()
        .take(visible)
        .enumerate()
        .map(|(index, named)| {
            let active = named.palette == theme::current();
            let prefix = if active { "●" } else { " " };

            ListItem::new(format!(
                "{} {}{}",
                prefix,
                named.name,
                if named.builtin { "  [built-in]" } else { "  [user]" },
            ))
            .style(if index == app.selected_theme_preset {
                theme::selected()
            } else {
                Style::default().fg(theme::text())
            })
        })
        .collect::<Vec<_>>();

    frame.render_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" NAMED THEMES ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::orange()))
                    .style(theme::panel()),
            ),
        preset_rows[0],
    );

    for (index, _) in app.theme_library.iter().take(visible).enumerate() {
        app.regions.theme_presets.push((
            index,
            Rect::new(
                preset_rows[0].x + 1,
                preset_rows[0].y + 1 + index as u16,
                preset_rows[0].width.saturating_sub(2),
                1,
            ),
        ));
    }

    frame.render_widget(
        Paragraph::new(format!(
            "ACTIVE: {}\nEDGES: {}\n\nChoose a named theme or edit individual color slots.",
            theme::current_theme_name(&app.theme_library),
            theme::edge_style().label(),
        ))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" THEME LIBRARY ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::yellow()))
                .style(theme::panel()),
        ),
        preset_rows[1],
    );

    let half = (ThemeSlot::ALL.len() + 1) / 2;
    render_theme_slot_column(frame, cols[1], &ThemeSlot::ALL[..half], app);
    render_theme_slot_column(frame, cols[2], &ThemeSlot::ALL[half..], app);
}

fn render_theme_slot_column(
    frame: &mut Frame,
    area: Rect,
    slots: &[ThemeSlot],
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            slots.iter().map(|_| Constraint::Length(3)).collect::<Vec<_>>(),
        )
        .split(area);

    for (slot, rect) in slots.iter().zip(rows.iter()) {
        let value = theme::slot_value(*slot);

        frame.render_widget(
            Paragraph::new(format!(
                " {}   {}   ■ ",
                slot.label(),
                value.hex()
            ))
            .style(
                Style::default()
                    .fg(value.color())
                    .bg(theme::panel_color())
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        Style::default().fg(theme::muted_color())
                    )
                    .style(theme::panel()),
            ),
            *rect,
        );

        app.regions.theme_slots.push((*slot, *rect));
    }
}

fn render_help(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(
r#"MOUSE
• Click deck names, dashboard tabs and action buttons.
• Click card names in CARDS, CUT/ADD, BRACKET, GOLDFISH, COMBOS and legality views.
• Click archetype tags to show matching cards and preview their art.
• Click storefront buttons in the preview pane.
• Mouse wheel navigates the deck/card browser.

DECK LIBRARY
N / NEW DECK    create/import/generate
R / REFRESH     rescan ./decks
U / UPDATE      refresh Scryfall cards, prices and purchase links

DASHBOARD
← → / H L       switch pages
S               50,000 resource simulations
G               refresh heuristic goldfish reference
GOLDFISH tab    interactive manual playtest sandbox
CUT / ADD       arrows navigate; Enter select; B sideboard
SIM             arrows highlight metrics; Enter reruns
COMBOS          arrows highlight combo cards; Enter preview
SOURCES         arrows select; Enter opens source URL
AI AGENT      D runs local research; arrows/Enter inspect cards
C               Commander Spellbook combos
B               switch mainboard / sideboard on CARDS
Esc             deck library
SETTINGS button appearance, price currency, and saved themes
COPY            export current decklist to clipboard
Q               quit

CREATE DECK
Paste List:
  Type a deck name, Enter, then paste the list.
  F2 or Ctrl+Enter saves.

Moxfield:
  Paste a public Moxfield deck URL. This is best-effort because the
  endpoint used by Moxfield is not documented as a stable public API.

Generate:
  Enter a commander name. Commander Lab builds a local optimized shell
  using color identity, functional-role targets, EDHREC rank from Scryfall,
  and EDHREC-style archetype-tag matching.

PRICES
Prices are Scryfall bulk-data snapshots refreshed by UPDATE. They are not
live checkout quotes. Storefront buttons open Scryfall-provided purchase URIs.

BRACKETS
Bracket detection is an estimate based on Game Changers, fast mana, tutors,
extra turns, land denial, and Commander Spellbook combo tags. Intent remains
part of the official bracket conversation."#,
        )
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" HELP / ABOUT ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::blue()))
                .style(theme::panel()),
        ),
        area,
    );
}

fn render_card_preview_panel(
    frame: &mut Frame,
    area: Rect,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(58),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    let preview_block = Block::default()
        .title(" CARD IMAGE ")
        .borders(Borders::ALL)
                    .border_type(theme::border_type())
        .border_style(Style::default().fg(theme::magenta()))
        .style(theme::panel());

    let image_area = preview_block.inner(rows[0]);
    frame.render_widget(preview_block, rows[0]);

    // Clear the full image region so terminal graphics cannot leave artifacts.
    frame.render_widget(Clear, image_area);
    frame.render_widget(
        Block::default().style(theme::panel()),
        image_area,
    );

    if let Some(protocol) = app.card_image.as_ref() {
        if let Some(placeholder_area) = protocol.needs_placeholder(image_area) {
            frame.render_widget(
                Paragraph::new(
                    "Image protocol cannot fit here.\nResize the terminal.",
                )
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false }),
                placeholder_area,
            );
        } else {
            frame.render_widget(
                TerminalImage::new(protocol).allow_clipping(true),
                image_area,
            );
        }
    } else {
        frame.render_widget(
            Paragraph::new(
                "Click a card name to preview it.",
            )
            .alignment(Alignment::Center),
            image_area,
        );
    }

    let details = if let Some(card) = app.selected_card_info.as_ref() {
        format!(
            "{}   {}\n{}\nMV {:.1}   Identity {}\nEDHREC rank {}\n{}\n\nPrices: {}",
            card.name,
            card.mana_cost,
            card.type_line,
            card.cmc,
            if card.color_identity.is_empty() {
                "C"
            } else {
                &card.color_identity
            },
            card.edhrec_rank
                .map(|rank| rank.to_string())
                .unwrap_or_else(|| "-".to_string()),
            card.oracle_text,
            price_line(card),
        )
    } else {
        "No card selected.".to_string()
    };

    frame.render_widget(
        Paragraph::new(details)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" CARD DATA / PRICE SNAPSHOT ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::cyan()))
                    .style(theme::panel()),
            ),
        rows[1],
    );

    let stores = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[2]);

    render_button(
        frame,
        stores[0],
        " TCGPLAYER ",
        UiAction::StoreTcgplayer,
        theme::blue(),
        app,
    );

    render_button(
        frame,
        stores[1],
        " CARDMARKET ",
        UiAction::StoreCardmarket,
        theme::green(),
        app,
    );

    if theme::price_currency() == theme::PriceCurrency::Tix {
        render_button(
            frame,
            stores[2],
            " CARDHOARDER ",
            UiAction::StoreCardhoarder,
            theme::purple(),
            app,
        );
    } else {
        frame.render_widget(
            Paragraph::new(" MTGO HIDDEN ")
                .alignment(Alignment::Center)
                .style(
                    Style::default()
                        .fg(theme::muted_color())
                        .bg(theme::panel_color()),
                ),
            stores[2],
        );
    }
}

fn format_price_value(
    value: Option<f64>,
    currency: theme::PriceCurrency,
) -> String {
    match (value, currency) {
        (Some(value), theme::PriceCurrency::Usd) => {
            format!("${value:.2}")
        }
        (Some(value), theme::PriceCurrency::Eur) => {
            format!("€{value:.2}")
        }
        (Some(value), theme::PriceCurrency::Tix) => {
            format!("{value:.2} tix")
        }
        (None, _) => "price -".to_string(),
    }
}

fn card_price_value(
    card: &crate::data::Card,
    currency: theme::PriceCurrency,
) -> Option<f64> {
    match currency {
        theme::PriceCurrency::Usd => card.price_usd,
        theme::PriceCurrency::Eur => card.price_eur,
        theme::PriceCurrency::Tix => card.price_tix,
    }
}

fn analysis_price_values(
    analysis: &crate::analysis::DeckAnalysis,
) -> (f64, f64) {
    match theme::price_currency() {
        theme::PriceCurrency::Usd => (
            analysis.total_price_usd,
            analysis.sideboard_price_usd,
        ),
        theme::PriceCurrency::Eur => (
            analysis.total_price_eur,
            analysis.sideboard_price_eur,
        ),
        theme::PriceCurrency::Tix => (
            analysis.total_price_tix,
            analysis.sideboard_price_tix,
        ),
    }
}

fn format_deck_price(value: f64) -> String {
    match theme::price_currency() {
        theme::PriceCurrency::Usd => format!("${value:.2}"),
        theme::PriceCurrency::Eur => format!("€{value:.2}"),
        theme::PriceCurrency::Tix => format!("{value:.2} tix"),
    }
}

fn card_short_price(app: &App, name: &str) -> String {
    match app.db.get_card(name) {
        Ok(Some(card)) => format_price_value(
            card_price_value(&card, theme::price_currency()),
            theme::price_currency(),
        ),
        _ => "price -".to_string(),
    }
}

fn price_line(card: &crate::data::Card) -> String {
    let currency = theme::price_currency();
    format!(
        "{} {}",
        currency.label(),
        format_price_value(
            card_price_value(card, currency),
            currency,
        ),
    )
}


fn render_named_card_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    cards: &[String],
    app: &mut App,
) {
    let visible_height = area.height.saturating_sub(2) as usize;

    let items = cards
        .iter()
        .take(visible_height)
        .map(|name| {
            ListItem::new(format!(
                "{}   {}",
                name,
                card_short_price(app, name)
            ))
        })
        .collect::<Vec<_>>();

    frame.render_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::blue()))
                    .style(theme::panel()),
            ),
        area,
    );

    for (index, name) in cards.iter().take(visible_height).enumerate() {
        app.regions.named_cards.push((
            name.clone(),
            Rect::new(
                area.x + 1,
                area.y + 1 + index as u16,
                area.width.saturating_sub(2),
                1,
            ),
        ));
    }
}


fn render_suggestion_detail_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    cards: &[(String, String)],
    kind: SuggestionKind,
    border: Color,
    focused_column: bool,
    selected_index: usize,
    app: &mut App,
) {
    let mut lines = Vec::<Line>::new();
    let mut row = area.y + 1;
    let bottom = area.y + area.height.saturating_sub(1);

    for (index, (name, detail)) in cards.iter().enumerate() {
        if row >= bottom {
            break;
        }

        lines.push(
            Line::from(format!(
                "{}   {}",
                name,
                card_short_price(app, name)
            ))
            .style(
                if focused_column && index == selected_index {
                    theme::selected()
                } else {
                    Style::default().fg(theme::text())
                },
            ),
        );

        let hit = Rect::new(
            area.x + 1,
            row,
            area.width.saturating_sub(2),
            1,
        );

        app.regions
            .suggestion_cards
            .push((kind, name.clone(), hit));
        app.regions.named_cards.push((name.clone(), hit));
        row += 1;

        for detail_line in detail.lines().take(2) {
            if row >= bottom {
                break;
            }

            lines.push(
                Line::from(format!("  {detail_line}"))
                    .style(Style::default().fg(theme::muted_color())),
            );
            row += 1;
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(border))
                    .style(theme::panel()),
            ),
        area,
    );
}


fn register_text_card_hits(
    app: &mut App,
    area: Rect,
    lines: &[String],
) {
    let deck_cards = app
        .parsed
        .as_ref()
        .map(|deck| {
            deck.commanders
                .iter()
                .chain(deck.main.iter())
                .chain(deck.sideboard.iter())
                .map(|entry| entry.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let max_rows = area.height.saturating_sub(2) as usize;

    for (index, line) in lines.iter().take(max_rows).enumerate() {
        if let Some(name) = deck_cards
            .iter()
            .filter(|name| line.contains(name.as_str()))
            .max_by_key(|name| name.len())
        {
            app.regions.named_cards.push((
                name.clone(),
                Rect::new(
                    area.x + 1,
                    area.y + 1 + index as u16,
                    area.width.saturating_sub(2),
                    1,
                ),
            ));
        }
    }
}






fn render_button(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    action: UiAction,
    color: Color,
    app: &mut App,
) {
    app.regions.buttons.push((action, area));

    frame.render_widget(
        Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(color)
                    .bg(theme::bg())
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(color)),
            ),
        area,
    );
}


fn render_tile(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    value: impl Into<String>,
    color: Color,
) {
    frame.render_widget(
        Paragraph::new(value.into())
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(color)
                    .bg(theme::panel_color())
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .title(format!(" {title} "))
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(color))
                    .style(theme::panel()),
            ),
        area,
    );
}

fn render_type_chart(
    frame: &mut Frame,
    area: Rect,
    analysis: &crate::analysis::DeckAnalysis,
) {
    frame.render_widget(
        Paragraph::new(format!(
            "L {}   C {}   A {}   E {}   I {}   S {}",
            analysis.types.lands,
            analysis.types.creatures,
            analysis.types.artifacts,
            analysis.types.enchantments,
            analysis.types.instants,
            analysis.types.sorceries,
        ))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" CARD TYPES ")
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(
                    Style::default().fg(theme::cyan()),
                )
                .style(theme::panel()),
        ),
        area,
    );
}

fn render_role_chart(
    frame: &mut Frame,
    area: Rect,
    analysis: &crate::analysis::DeckAnalysis,
) {
    frame.render_widget(
        Paragraph::new(format!(
            "Ramp {}   Draw {}   Int {}   Tut {}   Rec {}   Prot {}",
            analysis.roles.ramp,
            analysis.roles.draw,
            analysis.roles.removal + analysis.roles.counters,
            analysis.roles.tutors,
            analysis.roles.recursion,
            analysis.roles.protection,
        ))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" FUNCTIONAL ROLES ")
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(
                    Style::default().fg(theme::magenta()),
                )
                .style(theme::panel()),
        ),
        area,
    );
}


fn render_role_line_gauge(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    value: u32,
    target: u32,
    color: Color,
) {
    let ratio = (value as f64 / target as f64).clamp(0.0, 1.0);

    frame.render_widget(
        LineGauge::default()
            .block(
                Block::default()
                    .title(format!(" {label} "))
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(color))
                    .style(theme::panel()),
            )
            .label(format!("{value} / {target}"))
            .filled_style(
                Style::default()
                    .fg(color)
                    .add_modifier(Modifier::BOLD),
            )
            .unfilled_style(Style::default().fg(theme::panel_alt_color()))
            .ratio(ratio),
        area,
    );
}

fn render_pct_gauge(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    value: f64,
    color: Color,
    selected: bool,
) {
    frame.render_widget(
        Gauge::default()
            .block(
                Block::default()
                    .title(format!(" {label} "))
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(
                        if selected {
                            theme::selected()
                        } else {
                            Style::default().fg(color)
                        }
                    )
                    .style(theme::panel()),
            )
            .label(format!("{value:.1}%"))
            .gauge_style(
                Style::default()
                    .fg(color)
                    .bg(theme::panel_alt_color()),
            )
            .ratio((value / 100.0).clamp(0.0, 1.0))
            .use_unicode(true),
        area,
    );
}

fn bracket_color(bracket: u8) -> Color {
    match bracket {
        1 => theme::cyan(),
        2 => theme::green(),
        3 => theme::yellow(),
        4 => theme::orange(),
        _ => theme::magenta(),
    }
}

fn render_modal(frame: &mut Frame, app: &mut App) {
    // Modal controls own the click surface while the modal is open.
    app.regions.buttons.clear();
    app.regions.named_cards.clear();
    app.regions.archetype_tags.clear();
    app.regions.theme_slots.clear();
    app.regions.tabs.clear();
    app.regions.deck_list = None;
    app.regions.card_list = None;

    let area = centered_rect(74, 64, frame.area());

    frame.render_widget(Clear, area);

    // Clone modal state before mutable hitbox registration.
    match app.modal.clone() {
        Modal::CreateMenu { selected } => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(4),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Length(5),
                    Constraint::Min(2),
                    Constraint::Length(3),
                ])
                .split(area);

            frame.render_widget(
                Paragraph::new(
                    "Create or import a Commander deck. All results are saved as normal text decklists under ./decks.",
                )
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .title(" NEW DECK ")
                        .borders(Borders::ALL)
                    .border_type(theme::border_type())
                        .border_style(Style::default().fg(theme::green()))
                        .style(theme::panel()),
                ),
                rows[0],
            );

            render_modal_choice(
                frame,
                rows[1],
                "PASTE DECKLIST",
                "Paste almost any plaintext / Moxfield-style export",
                selected == 0,
                UiAction::CreatePaste,
                app,
            );

            render_modal_choice(
                frame,
                rows[2],
                "IMPORT MOXFIELD URL",
                "Best-effort import from a public Moxfield deck URL",
                selected == 1,
                UiAction::CreateMoxfield,
                app,
            );

            render_modal_choice(
                frame,
                rows[3],
                "IMPORT ARCHIDEKT URL",
                "Import a public Archidekt deck and preserve deck/sideboard categories",
                selected == 2,
                UiAction::CreateArchidekt,
                app,
            );

            render_modal_choice(
                frame,
                rows[4],
                "GENERATE OPTIMIZED",
                "Autocomplete commander → choose theme → choose bracket → coherent staged build",
                selected == 3,
                UiAction::CreateGenerate,
                app,
            );

            frame.render_widget(
                Paragraph::new(
                    "↑↓ select • Enter choose • Esc close",
                )
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme::muted_color())),
                rows[5],
            );

            render_button(
                frame,
                rows[6],
                " CANCEL ",
                UiAction::ModalCancel,
                theme::red(),
                app,
            );
        }

        Modal::PasteBody { buffer, scroll } => {
            render_paste_deck_modal(
                frame,
                area,
                &buffer,
                scroll,
                app,
            );
        }

        Modal::PasteCommander { deck: _, buffer } => {
            render_paste_commander_modal(
                frame,
                area,
                &buffer,
                app,
            );
        }

        Modal::PasteSaveName { deck, buffer } => {
            let fallback = deck
                .commanders
                .iter()
                .map(|entry| entry.name.clone())
                .collect::<Vec<_>>()
                .join(" + ");

            render_text_modal(
                frame,
                area,
                "NAME IMPORTED DECK",
                &format!(
                    "Blank uses: {}",
                    if fallback.is_empty() {
                        "Imported Deck"
                    } else {
                        fallback.as_str()
                    },
                ),
                &buffer,
                "Enter save • blank = commander name • Esc back",
            );
        }

        Modal::MoxfieldUrl { buffer } => {
            render_text_modal(
                frame,
                area,
                "IMPORT MOXFIELD",
                "Paste a public https://moxfield.com/decks/... URL.",
                &buffer,
                "Enter import • Esc back",
            );
        }

        Modal::ArchidektUrl { buffer } => {
            render_text_modal(
                frame,
                area,
                "IMPORT ARCHIDEKT",
                "Paste a public https://archidekt.com/decks/<id>/... URL.",
                &buffer,
                "Enter import • Esc back",
            );
        }


        Modal::GenerateCommander { buffer } => {
            render_generate_commander_modal(
                frame,
                area,
                &buffer,
                app,
            );
        }

        Modal::GenerateTheme {
            commander,
            themes,
            selected,
        } => {
            render_generation_theme_modal(
                frame,
                area,
                &commander,
                &themes,
                selected,
                app,
            );
        }

        Modal::GenerateBracket {
            commander,
            theme,
            selected,
        } => {
            render_generation_bracket_modal(
                frame,
                area,
                &commander,
                &theme,
                selected,
                app,
            );
        }

        Modal::GenerationProgress => {
            render_generation_progress_modal(frame, area, app);
        }

        Modal::DeleteDeckConfirm {
            deck_index: _,
            deck_name,
        } => {
            render_delete_deck_modal(frame, area, &deck_name, app);
        }

        Modal::ThemePicker {
            slot,
            value,
            channel,
        } => {
            render_theme_picker_modal(
                frame,
                area,
                slot,
                value,
                channel,
            );
        }

Modal::UserReferenceUrl { buffer } => {
    let popup = centered_rect(76, 38, area);
    frame.render_widget(Clear, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(popup);

    frame.render_widget(
        Paragraph::new(
            "Moxfield, Archidekt, or public reference URL",
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" ADD USER REFERENCE ")
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::yellow()))
                .style(theme::panel()),
        ),
        rows[0],
    );

    frame.render_widget(
        Paragraph::new(
            if buffer.is_empty() {
                "https://www.moxfield.com/decks/..."
                    .to_string()
            } else {
                buffer.clone()
            },
        )
        .wrap(Wrap { trim: false })
        .style(
            if buffer.is_empty() {
                Style::default().fg(theme::muted_color())
            } else {
                Style::default().fg(theme::text())
            },
        )
        .block(
            Block::default()
                .title(" REFERENCE URL ")
                .borders(Borders::ALL)
                .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::cyan()))
                .style(theme::panel()),
        ),
        rows[1],
    );

    frame.render_widget(
        Paragraph::new(
            "Enter import • paste supported • Esc cancel",
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme::muted_color())),
        rows[2],
    );
}

Modal::DeepLearnUpgradeBracket { selected } => {
    let popup = centered_rect(64, 36, area);
    frame.render_widget(Clear, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(5),
            Constraint::Min(3),
        ])
        .split(popup);

    frame.render_widget(
        Paragraph::new(format!(
            "Choose the desired Commander bracket for the upgraded copy.\n\nCurrent selection: Bracket {selected}\n\nThe original deck will not be overwritten."
        ))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" AI UPGRADE — TARGET BRACKET ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::yellow()))
                .style(theme::panel()),
        ),
        rows[0],
    );

    let choices = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ])
        .split(rows[1]);

    for (index, action) in [
        UiAction::DeepLearnUpgrade1,
        UiAction::DeepLearnUpgrade2,
        UiAction::DeepLearnUpgrade3,
        UiAction::DeepLearnUpgrade4,
        UiAction::DeepLearnUpgrade5,
    ]
    .into_iter()
    .enumerate()
    {
        let bracket = index as u8 + 1;

        render_button(
            frame,
            choices[index],
            &format!(" B{bracket} "),
            action,
            if bracket == selected {
                theme::yellow()
            } else {
                theme::muted_color()
            },
            app,
        );
    }

    frame.render_widget(
        Paragraph::new(
            "←/→ choose • 1–5 direct • Enter generate • Esc cancel",
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme::muted_color())),
        rows[2],
    );
}

        Modal::AiModelPicker { selected } => {
            let config = AiConfig::load_or_create();
            let presets = model_presets(config.provider);
            let popup = centered_rect(70, 52, area);

            frame.render_widget(Clear, popup);

            let mut constraints = vec![
                Constraint::Length(5),
            ];
            constraints.extend(
                (0..presets.len())
                    .map(|_| Constraint::Length(4)),
            );
            constraints.push(Constraint::Min(2));

            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(popup);

            frame.render_widget(
                Paragraph::new(format!(
                    "{} models\nCurrent: {}",
                    config.provider.label(),
                    config.model,
                ))
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .title(" SELECT AI MODEL ")
                        .borders(Borders::ALL)
                        .border_type(theme::border_type())
                        .border_style(
                            Style::default().fg(theme::magenta()),
                        )
                        .style(theme::panel()),
                ),
                rows[0],
            );

            let actions = [
                UiAction::SettingsAiModel0,
                UiAction::SettingsAiModel1,
                UiAction::SettingsAiModel2,
                UiAction::SettingsAiModel3,
            ];

            for (index, preset) in presets.iter().enumerate() {
                render_modal_choice(
                    frame,
                    rows[index + 1],
                    preset.name,
                    &format!(
                        "{} • {}",
                        preset.description,
                        preset.model,
                    ),
                    index == selected,
                    actions[index],
                    app,
                );
            }

            frame.render_widget(
                Paragraph::new(
                    "↑/↓ select • Enter use model • Esc cancel",
                )
                .alignment(Alignment::Center)
                .style(
                    Style::default().fg(theme::muted_color()),
                ),
                rows[rows.len() - 1],
            );
        }

        Modal::AiApiUrl { buffer } => {
            render_text_modal(
                frame,
                area,
                "AI API BASE URL",
                "OpenAI-compatible server base URL, e.g. https://api.openai.com",
                &buffer,
                "Enter save • paste supported • Esc cancel",
            );
        }

        Modal::AiApiModel { buffer } => {
            render_text_modal(
                frame,
                area,
                "AI API MODEL",
                "Enter the exact model identifier required by the provider.",
                &buffer,
                "Enter save • paste supported • Esc cancel",
            );
        }

        Modal::AiApiKey { buffer } => {
            let masked = if buffer.is_empty() {
                "Paste or type API key".to_string()
            } else {
                "•".repeat(buffer.chars().count().min(64))
            };

            let popup = centered_rect(74, 30, area);
            frame.render_widget(Clear, popup);

            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(7),
                    Constraint::Min(5),
                    Constraint::Length(3),
                ])
                .split(popup);

            frame.render_widget(
                Paragraph::new(
                    "The key is masked on screen and is not included in AI Activity or Agent logs.",
                )
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .title(" AI API KEY ")
                        .borders(Borders::ALL)
                        .border_type(theme::border_type())
                        .border_style(
                            Style::default().fg(theme::yellow()),
                        )
                        .style(theme::panel()),
                ),
                rows[0],
            );

            frame.render_widget(
                Paragraph::new(masked)
                    .wrap(Wrap { trim: false })
                    .block(
                        Block::default()
                            .title(" KEY ")
                            .borders(Borders::ALL)
                            .border_type(theme::border_type())
                            .border_style(
                                Style::default().fg(theme::cyan()),
                            )
                            .style(theme::panel()),
                    ),
                rows[1],
            );

            frame.render_widget(
                Paragraph::new(
                    "Enter save • paste supported • Esc cancel",
                )
                .alignment(Alignment::Center)
                .style(
                    Style::default().fg(theme::muted_color()),
                ),
                rows[2],
            );
        }

        Modal::ThemeName { buffer } => {
            render_text_modal(
                frame,
                area,
                "SAVE NAMED THEME",
                "Give the current palette a name. Reusing a user-theme name updates it.",
                &buffer,
                "Enter save • Esc cancel",
            );
        }

        Modal::None => {}
    }
}

fn render_theme_picker_modal(
    frame: &mut Frame,
    area: Rect,
    slot: ThemeSlot,
    value: theme::RgbValue,
    selected_channel: ThemeChannel,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Min(3),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(format!(
            "{}\n{}\nR {}   G {}   B {}",
            slot.label(),
            value.hex(),
            value.r,
            value.g,
            value.b
        ))
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(theme::text())
                .bg(value.color())
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .title(" RGB COLOR PICKER ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::blue())),
        ),
        rows[0],
    );

    for (index, channel) in ThemeChannel::ALL.iter().enumerate() {
        let amount = channel.value(value);
        let available = rows[index + 1].width.saturating_sub(20) as usize;
        let filled = if available == 0 {
            0
        } else {
            amount as usize * available / 255
        };

        let bar = format!(
            "{:<5} {:>3} [{}{}]",
            channel.label(),
            amount,
            "█".repeat(filled),
            " ".repeat(available.saturating_sub(filled)),
        );

        frame.render_widget(
            Paragraph::new(bar)
                .style(
                    Style::default()
                        .fg(theme::text())
                        .bg(channel.preview(amount))
                        .add_modifier(
                            if *channel == selected_channel {
                                Modifier::BOLD | Modifier::REVERSED
                            } else {
                                Modifier::empty()
                            },
                        ),
                )
                .block(Block::default().borders(Borders::ALL)
                    .border_type(theme::border_type())),
            rows[index + 1],
        );
    }

    frame.render_widget(
        Paragraph::new(
            "↑↓/Tab channel • ←→ ±1 • PgUp/PgDn ±16 • Home/End 0/255 • Enter/S save • Esc cancel",
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(theme::muted_color())),
        rows[4],
    );
}

fn render_generate_commander_modal(
    frame: &mut Frame,
    area: Rect,
    buffer: &str,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(
            "Type a commander approximately. Commander Lab will autocomplete, \
             correct likely misspellings, and detect compatible partner setups. \
             Use + between two commanders when you want a specific pair.",
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" COMMANDER ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::green()))
                .style(theme::panel()),
        ),
        rows[0],
    );

    frame.render_widget(
        Paragraph::new(buffer.to_string())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" NAME / PAIR ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::cyan()))
                    .style(theme::panel()),
            ),
        rows[1],
    );

    let (suggestions, autocomplete_pending) =
        app.commander_autocomplete(buffer);

    let text = if autocomplete_pending
        || suggestions.is_empty()
    {
        Vec::<Line>::new()
    } else {
        suggestions
            .iter()
            .enumerate()
            .map(|(index, name)| {
                Line::from(format!(
                    "{}. {}",
                    index + 1,
                    name,
                ))
                .style(
                    if app
                        .hovered_commander_suggestion
                        .as_ref()
                        .map(|hovered| {
                            hovered.eq_ignore_ascii_case(name)
                        })
                        .unwrap_or(false)
                    {
                        theme::selected()
                    } else {
                        Style::default().fg(theme::text())
                    },
                )
            })
            .collect::<Vec<_>>()
    };

    // Each commander suggestion owns its full inner row.
    if !autocomplete_pending {
        for (index, name) in suggestions.iter().enumerate() {
            let y = rows[2]
                .y
                .saturating_add(1)
                .saturating_add(index as u16);

            if y
                < rows[2]
                    .y
                    .saturating_add(rows[2].height.saturating_sub(1))
            {
                app.regions
                    .generate_commander_suggestions
                    .push((
                        name.clone(),
                        Rect::new(
                            rows[2].x.saturating_add(1),
                            y,
                            rows[2].width.saturating_sub(2),
                            1,
                        ),
                    ));
            }
        }
    }

    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" MATCHES ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::purple()))
                    .style(theme::panel()),
            ),
        rows[2],
    );

    render_button(
        frame,
        rows[3],
        " USE COMMANDER ",
        UiAction::GenerateCommanderUse,
        theme::green(),
        app,
    );

    frame.render_widget(
        Paragraph::new("Click match • Esc back")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::muted_color())),
        rows[4],
    );
}

fn render_generation_theme_modal(
    frame: &mut Frame,
    area: Rect,
    commander: &str,
    themes: &[String],
    selected: usize,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(format!(
            "{commander}\n\nChoose a primary deck theme. EDHREC's most popular \
             commander themes are shown first, with local inference filling \
             gaps when needed."
        ))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" CHOOSE DECK THEME — EDHREC TOP 10 TAGS ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::green()))
                .style(theme::panel()),
        ),
        rows[0],
    );

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(rows[1]);

    let actions = [
        UiAction::GenerateTheme0,
        UiAction::GenerateTheme1,
        UiAction::GenerateTheme2,
        UiAction::GenerateTheme3,
        UiAction::GenerateTheme4,
        UiAction::GenerateTheme5,
        UiAction::GenerateTheme6,
        UiAction::GenerateTheme7,
        UiAction::GenerateTheme8,
        UiAction::GenerateTheme9,
        UiAction::GenerateTheme10,
        UiAction::GenerateTheme11,
    ];

    for column in 0..2usize {
        let start_index = column * 6;
        let end_index = (start_index + 6).min(themes.len());

        if start_index >= end_index {
            continue;
        }

        let column_themes =
            &themes[start_index..end_index];

        let theme_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                column_themes
                    .iter()
                    .map(|_| Constraint::Length(4))
                    .collect::<Vec<_>>(),
            )
            .split(cols[column]);

        for (offset, theme_name) in
            column_themes.iter().enumerate()
        {
            let index = start_index + offset;

            render_modal_choice(
                frame,
                theme_rows[offset],
                theme_name,
                if index < 10 {
                    "EDHREC ranked tag • preference only • click to continue"
                } else {
                    "Local play-style preference • off-theme cards stay eligible"
                },
                selected == index,
                actions[index],
                app,
            );
        }
    }

    frame.render_widget(
        Paragraph::new(
            "↑↓ choose • Enter continue • click any theme • Esc commander"
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme::muted_color())),
        rows[2],
    );
}

fn render_generation_progress_modal(
    frame: &mut Frame,
    area: Rect,
    app: &App,
) {
    let progress = app
        .generation_progress
        .clone()
        .unwrap_or_else(|| crate::generator::GenerationProgress {
            percent: 1,
            step: "Starting".to_string(),
            detail: "Preparing generator worker".to_string(),
        });

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(5),
            Constraint::Min(10),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(format!(
            "{}%\n{}\n{}",
            progress.percent,
            progress.step,
            progress.detail
        ))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" GENERATING COMMANDER DECK ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::green()))
                .style(theme::panel()),
        ),
        rows[0],
    );

    frame.render_widget(
        Gauge::default()
            .block(
                Block::default()
                    .title(" PROGRESS ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::cyan())),
            )
            .gauge_style(
                Style::default()
                    .fg(theme::green())
                    .bg(theme::panel_alt_color())
                    .add_modifier(Modifier::BOLD),
            )
            .percent(progress.percent),
        rows[1],
    );

    // Generation diagnostics stay ephemeral in the progress view.
    let trace_height =
        rows[2].height.saturating_sub(2) as usize;

    let live_trace = if app.generation_trace.is_empty() {
        "Waiting for generator activity...".to_string()
    } else {
        let start = app
            .generation_trace
            .len()
            .saturating_sub(trace_height.max(1));

        app.generation_trace[start..].join("\n")
    };

    frame.render_widget(
        Paragraph::new(live_trace)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" LIVE GENERATION ACTIVITY ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::purple()))
                    .style(theme::panel()),
            ),
        rows[2],
    );
}

fn render_generation_bracket_modal(
    frame: &mut Frame,
    area: Rect,
    commander: &str,
    theme_name: &str,
    selected: u8,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Min(3),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(format!(
            "Generate around:\n{commander}\nTheme: {theme_name}\n\nChoose the intended Commander bracket."
        ))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" TARGET BRACKET ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::blue()))
                .style(theme::panel()),
        ),
        rows[0],
    );

    let options = [
        (
            1,
            "BRACKET 1 — EXHIBITION",
            "No Game Changers/fast combo package; synergy-first casual build.",
            UiAction::GenerateBracket1,
        ),
        (
            2,
            "BRACKET 2 — CORE",
            "Low tutor/fast-mana density; slower combo package where legal.",
            UiAction::GenerateBracket2,
        ),
        (
            3,
            "BRACKET 3 — UPGRADED",
            "Stronger efficiency with a focused combo and moderate tutors.",
            UiAction::GenerateBracket3,
        ),
        (
            4,
            "BRACKET 4 — OPTIMIZED",
            "Compact combos, stronger tutors/fast mana, high synergy density.",
            UiAction::GenerateBracket4,
        ),
        (
            5,
            "BRACKET 5 — cEDH PROFILE",
            "Maximum efficiency, compact wins, dense tutors and fast mana.",
            UiAction::GenerateBracket5,
        ),
    ];

    for (index, (bracket, title, description, action)) in
        options.into_iter().enumerate()
    {
        render_modal_choice(
            frame,
            rows[index + 1],
            title,
            description,
            selected == bracket,
            action,
            app,
        );
    }

    frame.render_widget(
        Paragraph::new("↑↓ / 1–5 select • Enter generate • Esc commander")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::muted_color())),
        rows[6],
    );
}

fn render_delete_deck_modal(
    frame: &mut Frame,
    area: Rect,
    deck_name: &str,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(format!(
            "Delete '{deck_name}' from the deck library?\n\n\
             This removes the deck text file from ./decks and also removes \
             its generated .plan.md file when present."
        ))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" DELETE DECK ")
                .borders(Borders::ALL)
                    .border_type(theme::border_type())
                .border_style(Style::default().fg(theme::red()))
                .style(theme::panel()),
        ),
        rows[0],
    );

    render_button(
        frame,
        rows[1],
        " YES — DELETE ",
        UiAction::ConfirmDeleteDeck,
        theme::red(),
        app,
    );

    render_button(
        frame,
        rows[2],
        " CANCEL ",
        UiAction::ModalCancel,
        theme::blue(),
        app,
    );
}

fn render_modal_choice(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    description: &str,
    selected: bool,
    action: UiAction,
    app: &mut App,
) {
    app.regions.buttons.push((action, area));

    frame.render_widget(
        Paragraph::new(format!("{title}\n{description}"))
            .alignment(Alignment::Center)
            .style(if selected {
                theme::selected()
            } else {
                theme::panel()
            })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(if selected {
                        theme::green()
                    } else {
                        theme::muted_color()
                    })),
            ),
        area,
    );
}

fn render_paste_deck_modal(
    frame: &mut Frame,
    area: Rect,
    buffer: &str,
    requested_scroll: usize,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(area);

    let all_lines = if buffer.is_empty() {
        vec![String::new()]
    } else {
        buffer.lines().map(str::to_string).collect::<Vec<_>>()
    };

    let visible = rows[0].height.saturating_sub(2) as usize;
    let max_scroll = all_lines.len().saturating_sub(visible);
    let scroll = if requested_scroll == usize::MAX {
        max_scroll
    } else {
        requested_scroll.min(max_scroll)
    };

    let shown = all_lines
        .iter()
        .skip(scroll)
        .take(visible.max(1))
        .cloned()
        .map(Line::from)
        .collect::<Vec<_>>();

    frame.render_widget(
        Paragraph::new(shown)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(format!(
                        " PASTE DECKLIST • {} LINES ",
                        all_lines.len(),
                    ))
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::cyan()))
                    .style(theme::panel()),
            ),
        rows[0],
    );

    render_button(
        frame,
        rows[1],
        " CONTINUE ",
        UiAction::PasteDeckContinue,
        theme::green(),
        app,
    );

    frame.render_widget(
        Paragraph::new(
            "Wheel / ↑↓ / PgUp PgDn scroll • Esc cancel",
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme::muted_color())),
        rows[2],
    );
}

fn render_paste_commander_modal(
    frame: &mut Frame,
    area: Rect,
    buffer: &str,
    app: &mut App,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(7),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(buffer.to_string())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" COMMANDER ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::cyan()))
                    .style(theme::panel()),
            ),
        rows[0],
    );

    let (suggestions, pending) =
        app.commander_autocomplete(buffer);

    let lines = if pending {
        Vec::<Line>::new()
    } else {
        suggestions
            .iter()
            .map(|name| {
                Line::from(name.clone()).style(
                    if app
                        .hovered_commander_suggestion
                        .as_ref()
                        .map(|hovered| {
                            hovered.eq_ignore_ascii_case(name)
                        })
                        .unwrap_or(false)
                    {
                        theme::selected()
                    } else {
                        Style::default().fg(theme::text())
                    },
                )
            })
            .collect::<Vec<_>>()
    };

    for (index, name) in suggestions.iter().enumerate() {
        let y = rows[1]
            .y
            .saturating_add(1)
            .saturating_add(index as u16);

        if y
            < rows[1]
                .y
                .saturating_add(rows[1].height.saturating_sub(1))
        {
            app.regions
                .generate_commander_suggestions
                .push((
                    name.clone(),
                    Rect::new(
                        rows[1].x.saturating_add(1),
                        y,
                        rows[1].width.saturating_sub(2),
                        1,
                    ),
                ));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" MATCHES ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::purple()))
                    .style(theme::panel()),
            ),
        rows[1],
    );

    render_button(
        frame,
        rows[2],
        " USE COMMANDER ",
        UiAction::PasteCommanderUse,
        theme::green(),
        app,
    );

    frame.render_widget(
        Paragraph::new("Click match • Esc back")
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::muted_color())),
        rows[3],
    );
}

fn render_text_modal(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    instructions: &str,
    buffer: &str,
    footer: &str,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(7),
            Constraint::Length(3),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(instructions)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(format!(" {title} "))
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::green()))
                    .style(theme::panel()),
            ),
        rows[0],
    );

    frame.render_widget(
        Paragraph::new(buffer.to_string())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" INPUT ")
                    .borders(Borders::ALL)
                    .border_type(theme::border_type())
                    .border_style(Style::default().fg(theme::cyan()))
                    .style(theme::panel()),
            ),
        rows[1],
    );

    frame.render_widget(
        Paragraph::new(footer)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::muted_color()))
            .block(Block::default().borders(Borders::ALL)
                    .border_type(theme::border_type())),
        rows[2],
    );
}

fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    area: Rect,
) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);

    horizontal[1]
}
