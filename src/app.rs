use crate::{
    ai::{model_presets, AiConfig, chat_with_deck_agent, AgentChatMessage},
    analysis::{analyze_deck, DeckAnalysis},
    archidekt::import_archidekt,
    archetype::tag_names_for_card,
    commander::{commander_name_suggestions, resolve_generation_commanders},
    bracket::refine_with_combo_report,
    data::{Card, CardDb},
    deep_agent::{
        run_optimization_agent, AgentProgress, DeepAgentResult,
        OptimizationGoal,
    },
    deep_learn::{
        deep_learn_deck, simulate_deep_learn_games,
        simulate_deep_learn_games_with_runs, DeepLearnCard,
        DeepLearnReport, DeepLearnTestSummary, TestingIntensity,
    },
    deck::{
        deck_to_text, load_deck_files, parse_deck_file, parse_deck_text,
        safe_deck_filename, write_deck_file, DeckFile, ParsedDeck,
    },
    deck_history::{
        append_run as append_deck_run, delete_history,
        load_history, render_history_lines, save_agent_session,
    },
    logging,
    generator::{
        generate_ai_upgrade, generate_optimized_deck,
        game_plan_markdown, suggest_generation_themes,
        GeneratedDeck, GenerationBracket, GenerationProgress,
    },
    moxfield::import_moxfield,
    runtime_stats::RuntimeStats,
    meta_sim::{simulate_meta_matchups, MetaMatchupSummary},
    references::{
        gather_reference_decks, invalidate_reference_snapshot,
        load_cached_meta_reference_decks,
    },
    simulation::{run_draw_simulation, DrawSimulation},
    spellbook::{find_my_combos, ComboReport},
    sources::{
        delete_sources_sidecar, load_sources_for_deck,
        save_sources_for_deck, sources_for_existing_deck, user_sources,
        DeckSource,
    },
    user_reference::{extract_urls, import_user_reference},
    theme::{self, NamedTheme, ThemeChannel, ThemeSlot},
    ui,
};
use anyhow::{anyhow, Context, Result};
use arboard::Clipboard;
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton,
    MouseEvent, MouseEventKind,
};
use image::ImageReader;
use ratatui::{
    layout::{Rect, Size},
    DefaultTerminal,
};
use ratatui_image::{
    picker::Picker,
    protocol::Protocol,
    Resize,
};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};
use regex::Regex;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

const IMAGE_USER_AGENT: &str =
    concat!("CommanderLab/", env!("CARGO_PKG_VERSION"), " local Commander deck analyzer");
const COMMANDER_AUTOCOMPLETE_DEBOUNCE_MS: u64 = 300;

enum GenerationEvent {
    Progress(GenerationProgress),
    Finished(Result<GeneratedDeck, String>),
}

enum DeepLearnEvent {
    ResearchProgress(String),
    ResearchFinished(Result<DeepLearnReport, String>),
    SimulationProgress(String),
    SimulationFinished(Result<DeepLearnTestSummary, String>),
    MetaSimulationProgress(String),
    MetaSimulationFinished(Result<MetaMatchupSummary, String>),
}

enum DeepAgentEvent {
    Progress(AgentProgress),
    Finished(Result<DeepAgentResult, String>),
}

#[derive(Debug, Clone)]
struct AgentInstructionPlan {
    research: bool,
    optimize: bool,
    simulate: bool,
    simulate_before_optimize: bool,
    meta_matchups: bool,
    exact_runs: Option<u32>,
    intensity: TestingIntensity,
    follow_up: bool,
    original: String,
}

impl AgentInstructionPlan {
    fn converse(original: String) -> Self {
        Self {
            research: false,
            optimize: false,
            simulate: false,
            simulate_before_optimize: false,
            meta_matchups: false,
            exact_runs: None,
            intensity: TestingIntensity::Thorough,
            follow_up: false,
            original,
        }
    }

    fn has_work(&self) -> bool {
        self.research || self.optimize || self.simulate
    }
}

enum AgentChatEvent {
    Finished(Result<String, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    DeckPicker,
    Dashboard,
    DeepLearn,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Theme,
    AiModel,
    Pricing,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 3] = [
        SettingsTab::Theme,
        SettingsTab::AiModel,
        SettingsTab::Pricing,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsTab::Theme => "THEME",
            SettingsTab::AiModel => "AI MODEL",
            SettingsTab::Pricing => "PRICING",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextPane {
    Log,
    Trace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Cards,
    Mana,
    Curve,
    Roles,
    Legality,
    Suggestions,
    Bracket,
    Simulation,
    Combos,
    Sources,
    Log,
    Help,
}

impl Page {
    pub const ALL: [Page; 13] = [
        Page::Overview,
        Page::Cards,
        Page::Mana,
        Page::Curve,
        Page::Roles,
        Page::Legality,
        Page::Suggestions,
        Page::Bracket,
        Page::Simulation,
        Page::Combos,
        Page::Sources,
        Page::Log,
        Page::Help,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::Overview => "OVERVIEW",
            Page::Cards => "CARDS",
            Page::Mana => "MANA",
            Page::Curve => "CURVE",
            Page::Roles => "ROLES",
            Page::Legality => "LEGALITY",
            Page::Suggestions => "CUT / ADD",
            Page::Bracket => "BRACKET",
            Page::Simulation => "SIM",
            Page::Combos => "COMBOS",
            Page::Sources => "SOURCES",
            Page::Log => "LOG",
            Page::Help => "HELP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepLearnPage {
    Overview,
    Agent,
    Chat,
    Research,
    Simulate,
    WinSpeed,
    Upgrade,
    Recommendations,
    Compare,
    Sources,
    Trace,
}

impl DeepLearnPage {
    pub const ALL: [DeepLearnPage; 11] = [
        DeepLearnPage::Overview,
        DeepLearnPage::Agent,
        DeepLearnPage::Chat,
        DeepLearnPage::Research,
        DeepLearnPage::Simulate,
        DeepLearnPage::WinSpeed,
        DeepLearnPage::Upgrade,
        DeepLearnPage::Recommendations,
        DeepLearnPage::Compare,
        DeepLearnPage::Sources,
        DeepLearnPage::Trace,
    ];

    pub fn title(self) -> &'static str {
        match self {
            DeepLearnPage::Overview => "OVERVIEW",
            DeepLearnPage::Agent => "AGENT",
            DeepLearnPage::Chat => "CHAT",
            DeepLearnPage::Research => "RESEARCH",
            DeepLearnPage::Simulate => "SIMULATE",
            DeepLearnPage::WinSpeed => "SPEED",
            DeepLearnPage::Upgrade => "UPGRADE",
            DeepLearnPage::Recommendations => "RECOMMEND",
            DeepLearnPage::Compare => "COMPARE",
            DeepLearnPage::Sources => "SOURCES",
            DeepLearnPage::Trace => "TRACE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAction {
    Update,
    RefreshDecks,
    NewDeck,
    DeleteDeck,
    ConfirmDeleteDeck,
    Simulate,
    Combos,
    Decks,
    Quit,
    Mainboard,
    Sideboard,
    StoreTcgplayer,
    StoreCardmarket,
    StoreCardhoarder,
    CreatePaste,
    CreateMoxfield,
    CreateArchidekt,
    CreateGenerate,
    GenerateBracket1,
    GenerateBracket2,
    GenerateBracket3,
    GenerateBracket4,
    GenerateBracket5,
    GenerateTheme0,
    GenerateTheme1,
    GenerateTheme2,
    GenerateTheme3,
    GenerateTheme4,
    GenerateTheme5,
    GenerateTheme6,
    GenerateTheme7,
    GenerateTheme8,
    GenerateTheme9,
    GenerateTheme10,
    GenerateTheme11,
    SettingsOpen,
    SettingsTabTheme,
    SettingsTabAiModel,
    SettingsTabPricing,
    SettingsCycleAiProvider,
    SettingsOpenModelPicker,
    SettingsAiModel0,
    SettingsAiModel1,
    SettingsAiModel2,
    SettingsAiModel3,
    SettingsEditApiUrl,
    SettingsEditApiKey,
    SettingsToggleWebResearch,
    ThemeToggleTransparency,
    ThemeSave,
    ThemeReset,
    ThemeToggleEdges,
    SettingsCycleCurrency,
    SideboardSuggestion,
    CopyDecklist,
    RunDeepLearn,
    OpenSelectedCombo,
    ThemeSaveNamed,
    AnalyzeSelected,
    OpenCardsPage,
    OpenBracketPage,
    OpenCombosPage,
    OpenDeepLearnPage,
    OpenAgentChatPage,
    PickerDeepLearn,
    DeepLearnRunResearch,
    DeepLearnRunSimulation,
    DeepAgentRun,
    DeepAgentSaveBest,
    DeepAgentChatSend,
    DeepAgentChatClear,
    PasteDeckContinue,
    PasteCommanderUse,
    GenerateCommanderUse,
    AddUserReference,
    RemoveUserReference,
    DeepLearnCopyDeck,
    DeepLearnUpgrade,
    DeepLearnUpgrade1,
    DeepLearnUpgrade2,
    DeepLearnUpgrade3,
    DeepLearnUpgrade4,
    DeepLearnUpgrade5,
    ModalCancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    Cut,
    Add,
}

#[derive(Debug, Clone)]
pub struct SuggestionSelection {
    pub kind: SuggestionKind,
    pub card: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardZone {
    Mainboard,
    Sideboard,
}

#[derive(Debug, Clone)]
pub enum Modal {
    None,
    CreateMenu { selected: usize },
    PasteBody {
        buffer: String,
        scroll: usize,
    },
    PasteCommander {
        deck: ParsedDeck,
        buffer: String,
    },
    PasteSaveName {
        deck: ParsedDeck,
        buffer: String,
    },
    MoxfieldUrl { buffer: String },
    ArchidektUrl { buffer: String },
    GenerateCommander { buffer: String },
    GenerateTheme {
        commander: String,
        themes: Vec<String>,
        selected: usize,
    },
    GenerateBracket {
        commander: String,
        theme: String,
        selected: u8,
    },
    GenerationProgress,
    DeleteDeckConfirm { deck_index: usize, deck_name: String },
    ThemePicker {
        slot: ThemeSlot,
        value: theme::RgbValue,
        channel: ThemeChannel,
    },
    ThemeName { buffer: String },
    AiModelPicker { selected: usize },
    AiApiUrl { buffer: String },
    AiApiModel { buffer: String },
    AiApiKey { buffer: String },
    DeepLearnUpgradeBracket { selected: u8 },
    UserReferenceUrl {
        buffer: String,
    },
}

impl Modal {
    pub fn active(&self) -> bool {
        !matches!(self, Modal::None)
    }
}

#[derive(Default, Clone)]
pub struct UiRegions {
    pub deck_list: Option<Rect>,
    pub deck_rows: Vec<(usize, Rect)>,
    pub card_list: Option<Rect>,
    pub tabs: Vec<(Page, Rect)>,
    pub buttons: Vec<(UiAction, Rect)>,
    pub named_cards: Vec<(String, Rect)>,
    pub archetype_tags: Vec<(String, Rect)>,
    pub theme_slots: Vec<(ThemeSlot, Rect)>,
    pub theme_presets: Vec<(usize, Rect)>,
    pub combo_rows: Vec<(usize, Rect)>,
    pub suggestion_cards: Vec<(SuggestionKind, String, Rect)>,
    pub source_links: Vec<(usize, String, Rect)>,
    pub source_rows: Vec<(usize, Rect)>,
    pub text_rows: Vec<(TextPane, usize, Rect)>,
    pub deep_learn_tabs: Vec<(DeepLearnPage, Rect)>,
    pub deep_learn_cards: Vec<(usize, String, Rect)>,
    pub generate_commander_suggestions: Vec<(String, Rect)>,
    pub agent_prompt_box: Option<Rect>,
}

#[derive(Debug, Clone)]
pub struct DeckComparisonEntry {
    pub quantity: u32,
    pub name: String,
    pub zone: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeckComparison {
    pub original_name: String,
    pub optimized_name: String,
    pub added: Vec<DeckComparisonEntry>,
    pub removed: Vec<DeckComparisonEntry>,
    pub unchanged_main: usize,
    pub original_main_count: u32,
    pub optimized_main_count: u32,
}

pub struct App {
    pub db: CardDb,
    pub decks: Vec<DeckFile>,
    pub selected_deck: usize,
    pub screen: Screen,
    pub page: Page,
    pub parsed: Option<ParsedDeck>,
    pub analysis: Option<DeckAnalysis>,
    pub simulation: Option<DrawSimulation>,
    pub combos: Option<ComboReport>,
    pub selected_archetype_tag: Option<String>,
    pub selected_suggestion: Option<SuggestionSelection>,
    pub deck_sources: Vec<DeckSource>,
    pub selected_source: usize,
    pub source_scroll: usize,
    pub trace_scroll: u16,
    pub agent_page_scroll: u16,
    pub agent_chat_scroll: usize,
    pub deck_history_lines: Vec<String>,
    pub deck_log_scroll: u16,
    pub hovered_text: Option<(TextPane, usize)>,
    pub deep_learn: Option<DeepLearnReport>,
    pub deep_learn_testing: Option<DeepLearnTestSummary>,
    pub meta_matchup: Option<MetaMatchupSummary>,
    pub deep_learn_page: DeepLearnPage,
    pub deep_learn_status: String,
    pub deep_learn_selected: usize,
    pub deep_learn_intensity: TestingIntensity,
    pub deep_learn_upgrade_bracket: u8,
    pub deep_learn_trace: Vec<String>,
    deep_learn_rx: Option<Receiver<DeepLearnEvent>>,
    pub deep_agent_goal: OptimizationGoal,
    pub deep_agent_rounds: u16,
    pub deep_agent_result: Option<DeepAgentResult>,
    pub deep_agent_comparison: Option<DeckComparison>,
    pub compare_scroll: usize,
    pub deep_agent_trace: Vec<String>,
    pub deep_agent_progress: u8,
    pub deep_agent_current_task: String,
    pub deep_agent_completed_tasks: Vec<String>,
    pub deep_agent_style_brief: String,
    pub deep_agent_last_brief: String,
    pub deep_agent_prompt_editing: bool,
    pub deep_agent_prompt_scroll: usize,
    deep_agent_rx: Option<Receiver<DeepAgentEvent>>,
    pub agent_chat_messages: Vec<AgentChatMessage>,
    pub agent_chat_input: String,
    pub agent_chat_status: String,
    agent_chat_rx: Option<Receiver<AgentChatEvent>>,
    pub agent_chat_optimize_after_research: bool,
    pub agent_chat_simulate_after_research: bool,
    pub agent_chat_simulate_after_optimize: bool,
    pub agent_chat_optimize_after_simulation: bool,
    pub agent_chat_pending_runs: Option<u32>,
    pub agent_chat_pending_intensity: TestingIntensity,
    pub agent_chat_pending_meta: bool,
    pub agent_chat_follow_up: Option<String>,
    pub theme_library: Vec<NamedTheme>,
    pub selected_theme_preset: usize,
    pub settings_tab: SettingsTab,
    pub suggestion_focus_column: usize,
    pub suggestion_focus_index: usize,
    pub combo_focus_index: usize,
    pub simulation_focus_index: usize,
    pub status: String,
    pub should_quit: bool,
    pub runtime_stats: RuntimeStats,

    pub card_zone: CardZone,
    pub selected_card: usize,
    pub card_scroll: usize,
    pub selected_card_info: Option<Card>,
    pub card_image: Option<Protocol>,
    pub image_picker: Picker,
    pub safe_image_mode: bool,

    pub modal: Modal,
    pub generation_progress: Option<GenerationProgress>,
    pub generation_trace: Vec<String>,
    generation_rx: Option<Receiver<GenerationEvent>>,

    commander_autocomplete_input: String,
    commander_autocomplete_results: Vec<String>,
    commander_autocomplete_last_edit: Option<Instant>,
    commander_autocomplete_ready: bool,
    pub hovered_commander_suggestion: Option<String>,

    pub regions: UiRegions,
}

fn push_trace_log(
    trace: &mut Vec<String>,
    scope: &str,
    message: impl AsRef<str>,
) {
    let line = logging::record(scope, message.as_ref());
    trace.push(line);

    if trace.len() > 500 {
        let remove = trace.len() - 500;
        trace.drain(0..remove);
    }
}

fn push_ui_trace(
    trace: &mut Vec<String>,
    scope: &str,
    message: impl AsRef<str>,
) {
    // Generation progress is intentionally in-memory only.
    let line = logging::line(scope, message.as_ref());
    trace.push(line);

    if trace.len() > 500 {
        let remove = trace.len() - 500;
        trace.drain(0..remove);
    }
}

impl App {
    pub fn new() -> Result<Self> {
        let _ = theme::load_saved_theme("./data/theme.json");
        let _ = theme::load_ui_style("./data/ui_style.json");
        let db = CardDb::open("./data/cards.sqlite")?;
        let decks = load_deck_files("./decks")?;

        let image_picker =
            Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

        let status = if db.has_cards() {
            format!(
                "Local card database ready: {} Oracle cards",
                db.card_count().unwrap_or(0)
            )
        } else {
            "Card database is empty. Click UPDATE or press U to download Scryfall data."
                .to_string()
        };

        Ok(Self {
            db,
            decks,
            selected_deck: 0,
            screen: Screen::DeckPicker,
            page: Page::Overview,
            parsed: None,
            analysis: None,
            simulation: None,
            combos: None,
            selected_archetype_tag: None,
            selected_suggestion: None,
            deck_sources: Vec::new(),
            selected_source: 0,
            source_scroll: 0,
            trace_scroll: 0,
            agent_page_scroll: 0,
            agent_chat_scroll: 0,
            deck_history_lines: Vec::new(),
            deck_log_scroll: 0,
            hovered_text: None,
            deep_learn: None,
            deep_learn_testing: None,
            meta_matchup: None,
            deep_learn_page: DeepLearnPage::Overview,
            deep_learn_status: "AI Agent is ready.".to_string(),
            deep_learn_selected: 0,
            deep_learn_intensity: TestingIntensity::Thorough,
            deep_learn_upgrade_bracket: 3,
            deep_learn_trace: Vec::new(),
            deep_learn_rx: None,
            deep_agent_goal: OptimizationGoal::FastWin,
            deep_agent_rounds: 3,
            deep_agent_result: None,
            deep_agent_comparison: None,
            compare_scroll: 0,
            deep_agent_trace: Vec::new(),
            deep_agent_progress: 0,
            deep_agent_current_task: "Waiting for Agent brief.".to_string(),
            deep_agent_completed_tasks: Vec::new(),
            deep_agent_style_brief: String::new(),
            deep_agent_last_brief: String::new(),
            deep_agent_prompt_editing: false,
            deep_agent_prompt_scroll: 0,
            deep_agent_rx: None,
            agent_chat_messages: Vec::new(),
            agent_chat_input: String::new(),
            agent_chat_status: "Chat with the Agent to refine the deck. Say \"run optimization\" to apply changes or ask for the decklist anytime.".to_string(),
            agent_chat_rx: None,
            agent_chat_optimize_after_research: false,
            agent_chat_simulate_after_research: false,
            agent_chat_simulate_after_optimize: false,
            agent_chat_optimize_after_simulation: false,
            agent_chat_pending_runs: None,
            agent_chat_pending_intensity: TestingIntensity::Thorough,
            agent_chat_pending_meta: false,
            agent_chat_follow_up: None,
            theme_library: theme::load_named_themes("./data/themes.json"),
            selected_theme_preset: 0,
            settings_tab: SettingsTab::Theme,
            suggestion_focus_column: 0,
            suggestion_focus_index: 0,
            combo_focus_index: 0,
            simulation_focus_index: 0,
            status,
            should_quit: false,
            runtime_stats: RuntimeStats::new(),
            card_zone: CardZone::Mainboard,
            selected_card: 0,
            card_scroll: 0,
            selected_card_info: None,
            card_image: None,
            image_picker,
            safe_image_mode: false,
            modal: Modal::None,
            generation_progress: None,
            generation_trace: Vec::new(),
            generation_rx: None,
            commander_autocomplete_input: String::new(),
            commander_autocomplete_results: Vec::new(),
            commander_autocomplete_last_edit: None,
            commander_autocomplete_ready: false,
            hovered_commander_suggestion: None,
            regions: UiRegions::default(),
        })
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            self.poll_generation_events()?;
            self.poll_deep_learn_events()?;
            self.poll_deep_agent_events()?;
            self.poll_agent_chat_events()?;

            // Restore the negotiated image protocol after workers stop.
            let safe_images = self.background_worker_active();
            self.set_safe_image_mode(safe_images);
            self.runtime_stats.refresh_if_due();

            terminal.draw(|frame| ui::render(frame, &mut self))?;

            if event::poll(Duration::from_millis(50))? {
                self.handle_event()?;
            }
        }

        Ok(())
    }

    fn handle_event(&mut self) -> Result<()> {
        match event::read()? {
            Event::Paste(text) if self.modal.active() => {
                self.handle_modal_paste(&text);
            }

            Event::Paste(text)
                if self.screen == Screen::DeepLearn
                    && self.deep_learn_page == DeepLearnPage::Agent
                    && self.deep_agent_prompt_editing =>
            {
                self.deep_agent_style_brief.push_str(&text);
                self.deep_agent_prompt_scroll = usize::MAX;
            }

            Event::Paste(text)
                if self.screen == Screen::DeepLearn
                    && self.deep_learn_page == DeepLearnPage::Chat =>
            {
                self.agent_chat_input.push_str(&text);
            }

            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if self.modal.active() {
                    self.handle_modal_key(key)?;
                } else {
                    match self.screen {
                        Screen::DeckPicker => self.handle_picker_key(key)?,
                        Screen::Dashboard => self.handle_dashboard_key(key)?,
                        Screen::DeepLearn => self.handle_deep_learn_key(key)?,
                        Screen::Settings => self.handle_theme_key(key)?,
                    }
                }
            }

            Event::Mouse(mouse) => self.handle_mouse(mouse)?,

            _ => {}
        }

        Ok(())
    }

    fn handle_picker_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.prev_deck(),
            KeyCode::Down | KeyCode::Char('j') => self.next_deck(),
            KeyCode::Char('r') | KeyCode::Char('R') => self.refresh_decks()?,
            KeyCode::Char('u') | KeyCode::Char('U') => self.update_database(),
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.modal = Modal::CreateMenu { selected: 0 };
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                self.screen = Screen::Settings;
            }
            KeyCode::Delete | KeyCode::Char('d') | KeyCode::Char('D') => {
                self.open_delete_deck_modal();
            }
            KeyCode::Enter => self.analyze_selected()?,
            _ => {}
        }

        Ok(())
    }


fn handle_dashboard_key(&mut self, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => self.should_quit = true,
        KeyCode::Esc => self.screen = Screen::DeckPicker,

        // H/L always change tabs. Arrow keys are reserved for keyboard
        // focus inside interactive analysis pages.
        KeyCode::Char('h') => self.prev_page(),
        KeyCode::Char('l') => self.next_page(),

        KeyCode::Left if self.page == Page::Suggestions => {
            self.suggestion_focus_column = 0;
            self.suggestion_focus_index = 0;
            self.preview_focused_suggestion();
        }
        KeyCode::Right if self.page == Page::Suggestions => {
            self.suggestion_focus_column = 1;
            self.suggestion_focus_index = 0;
            self.preview_focused_suggestion();
        }
        KeyCode::Left => self.prev_page(),
        KeyCode::Right => self.next_page(),

        KeyCode::Up | KeyCode::Char('k') if self.page == Page::Cards => {
            self.prev_card()
        }
        KeyCode::Down | KeyCode::Char('j') if self.page == Page::Cards => {
            self.next_card()
        }

        KeyCode::Up | KeyCode::Char('k')
            if self.page == Page::Suggestions =>
        {
            self.prev_suggestion_focus();
        }
        KeyCode::Down | KeyCode::Char('j')
            if self.page == Page::Suggestions =>
        {
            self.next_suggestion_focus();
        }
        KeyCode::Enter if self.page == Page::Suggestions => {
            self.select_focused_suggestion();
        }
        KeyCode::Char('b') | KeyCode::Char('B')
            if self.page == Page::Suggestions =>
        {
            self.select_focused_suggestion();
            self.sideboard_selected_suggestion()?;
        }

        KeyCode::Up | KeyCode::Char('k')
            if self.page == Page::Combos =>
        {
            self.prev_combo_focus();
        }
        KeyCode::Down | KeyCode::Char('j')
            if self.page == Page::Combos =>
        {
            self.next_combo_focus();
        }
        KeyCode::Enter if self.page == Page::Combos => {
            self.preview_focused_combo_card();
        }
        KeyCode::Char('o') | KeyCode::Char('O')
            if self.page == Page::Combos =>
        {
            self.open_selected_combo();
        }

        KeyCode::Up | KeyCode::Char('k')
            if self.page == Page::Simulation =>
        {
            self.simulation_focus_index =
                self.simulation_focus_index.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j')
            if self.page == Page::Simulation =>
        {
            self.simulation_focus_index =
                (self.simulation_focus_index + 1).min(4);
        }
        KeyCode::Enter if self.page == Page::Simulation => {
            self.run_simulation()?;
        }

        KeyCode::Up | KeyCode::Char('k')
            if self.page == Page::Sources =>
        {
            self.selected_source =
                self.selected_source.saturating_sub(1);
            self.source_scroll =
                self.source_scroll.min(self.selected_source);
        }
        KeyCode::Down | KeyCode::Char('j')
            if self.page == Page::Sources =>
        {
            if !self.deck_sources.is_empty() {
                self.selected_source =
                    (self.selected_source + 1)
                        .min(self.deck_sources.len() - 1);
                if self.selected_source
                    >= self.source_scroll.saturating_add(6)
                {
                    self.source_scroll =
                        self.selected_source.saturating_sub(5);
                }
            }
        }
        KeyCode::Enter if self.page == Page::Sources => {
            self.open_selected_source();
        }

        KeyCode::Char('+') if self.page == Page::Sources => {
            self.modal = Modal::UserReferenceUrl {
                buffer: String::new(),
            };
        }

        KeyCode::Delete if self.page == Page::Sources => {
            self.remove_selected_user_reference()?;
        }

        KeyCode::PageUp if self.page == Page::Log => {
            self.deck_log_scroll =
                self.deck_log_scroll.saturating_sub(8);
        }

        KeyCode::PageDown if self.page == Page::Log => {
            self.deck_log_scroll =
                self.deck_log_scroll.saturating_add(8);
        }

        KeyCode::Char('b') | KeyCode::Char('B')
            if self.page == Page::Cards =>
        {
            self.toggle_card_zone();
        }

        KeyCode::Char('u') | KeyCode::Char('U') => self.update_database(),
        KeyCode::Char('s') | KeyCode::Char('S') => self.run_simulation()?,
        KeyCode::Char('c') | KeyCode::Char('C') => self.run_combo_check()?,
        KeyCode::Char('r') | KeyCode::Char('R') => self.reanalyze(),
        _ => {}
    }

    Ok(())
}

    fn handle_deep_learn_key(&mut self, key: KeyEvent) -> Result<()> {
    if self.deep_learn_page == DeepLearnPage::Agent
        && self.deep_agent_prompt_editing
        && self.deep_agent_rx.is_none()
    {
        match key.code {
            KeyCode::Esc => {
                self.deep_agent_prompt_editing = false;
            }
            KeyCode::Enter => {
                let brief =
                    self.deep_agent_style_brief.trim().to_string();

                if !brief.is_empty() {
                    self.deep_agent_prompt_editing = false;
                    self.deep_agent_prompt_scroll = 0;
                    self.start_deep_agent_with_style(brief)?;
                }
            }
            KeyCode::Backspace => {
                self.deep_agent_style_brief.pop();
                self.deep_agent_prompt_scroll = usize::MAX;
            }
            KeyCode::PageUp | KeyCode::Up => {
                self.deep_agent_prompt_scroll =
                    self.deep_agent_prompt_scroll.saturating_sub(3);
            }
            KeyCode::PageDown | KeyCode::Down => {
                self.deep_agent_prompt_scroll =
                    self.deep_agent_prompt_scroll.saturating_add(3);
            }
            KeyCode::Char(ch) => {
                self.deep_agent_style_brief.push(ch);
                self.deep_agent_prompt_scroll = usize::MAX;
            }
            _ => {}
        }

        return Ok(());
    }

    if self.deep_learn_page == DeepLearnPage::Chat {
        return self.handle_agent_chat_key(key);
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            self.should_quit = true;
        }
        KeyCode::Esc => {
            self.screen = Screen::Dashboard;
        }

        KeyCode::Char('h') | KeyCode::Left => {
            let current = DeepLearnPage::ALL
                .iter()
                .position(|page| *page == self.deep_learn_page)
                .unwrap_or(0);

            self.deep_learn_page =
                DeepLearnPage::ALL[current.saturating_sub(1)];

            if self.deep_learn_page == DeepLearnPage::Chat {
                self.ensure_agent_chat_greeting();
            }
        }

        KeyCode::Char('l') | KeyCode::Right => {
            let current = DeepLearnPage::ALL
                .iter()
                .position(|page| *page == self.deep_learn_page)
                .unwrap_or(0);

            self.deep_learn_page = DeepLearnPage::ALL[
                (current + 1).min(DeepLearnPage::ALL.len() - 1)
            ];

            if self.deep_learn_page == DeepLearnPage::Chat {
                self.ensure_agent_chat_greeting();
            }
        }

        KeyCode::Char('r') | KeyCode::Char('R') => {
            self.start_deep_learn()?;
            self.deep_learn_page = DeepLearnPage::Research;
        }

        KeyCode::Char('s') | KeyCode::Char('S') => {
            self.start_deep_learn_simulation()?;
            self.deep_learn_page = DeepLearnPage::Simulate;
        }

        KeyCode::Char('c') | KeyCode::Char('C') => {
            self.copy_current_deck_to_clipboard();
        }

        KeyCode::Char('u') | KeyCode::Char('U') => {
            self.modal = Modal::DeepLearnUpgradeBracket {
                selected: self.deep_learn_upgrade_bracket,
            };
        }

        KeyCode::Up | KeyCode::Char('k')
            if self.deep_learn_page == DeepLearnPage::Sources =>
        {
            self.selected_source =
                self.selected_source.saturating_sub(1);
        }

        KeyCode::Down | KeyCode::Char('j')
            if self.deep_learn_page == DeepLearnPage::Sources =>
        {
            if !self.deck_sources.is_empty() {
                self.selected_source =
                    (self.selected_source + 1)
                        .min(self.deck_sources.len() - 1);
            }
        }

        KeyCode::Enter
            if self.deep_learn_page == DeepLearnPage::Sources =>
        {
            self.open_selected_source();
        }

        KeyCode::Char('+')
            if self.deep_learn_page == DeepLearnPage::Sources =>
        {
            self.modal = Modal::UserReferenceUrl {
                buffer: String::new(),
            };
        }

        KeyCode::Delete
            if self.deep_learn_page == DeepLearnPage::Sources =>
        {
            self.remove_selected_user_reference()?;
        }

        KeyCode::PageUp
            if self.deep_learn_page == DeepLearnPage::Compare =>
        {
            self.compare_scroll =
                self.compare_scroll.saturating_sub(8);
        }

        KeyCode::PageDown
            if self.deep_learn_page == DeepLearnPage::Compare =>
        {
            self.compare_scroll =
                self.compare_scroll.saturating_add(8);
        }

        KeyCode::PageUp
            if self.deep_learn_page == DeepLearnPage::Agent =>
        {
            self.agent_page_scroll =
                self.agent_page_scroll.saturating_sub(8);
        }

        KeyCode::PageDown
            if self.deep_learn_page == DeepLearnPage::Agent =>
        {
            self.agent_page_scroll =
                self.agent_page_scroll.saturating_add(8);
        }

        KeyCode::PageUp
            if self.deep_learn_page == DeepLearnPage::Trace =>
        {
            self.trace_scroll =
                self.trace_scroll.saturating_sub(8);
        }

        KeyCode::PageDown
            if self.deep_learn_page == DeepLearnPage::Trace =>
        {
            self.trace_scroll =
                self.trace_scroll.saturating_add(8);
        }

        KeyCode::Char('a') | KeyCode::Char('A')
            if self.deep_learn_page != DeepLearnPage::Chat =>
        {
            self.open_agent_chat_tab();
        }





        KeyCode::Up | KeyCode::Char('k')
            if self.deep_learn_page
                == DeepLearnPage::Recommendations =>
        {
            self.deep_learn_selected =
                self.deep_learn_selected.saturating_sub(1);
            self.preview_deep_learn_card();
        }

        KeyCode::Down | KeyCode::Char('j')
            if self.deep_learn_page
                == DeepLearnPage::Recommendations =>
        {
            self.next_deep_learn_focus();
        }

        KeyCode::Enter
            if self.deep_learn_page
                == DeepLearnPage::Recommendations =>
        {
            self.preview_deep_learn_card();
        }

        _ => {}
    }

    Ok(())
}

fn handle_theme_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.screen = Screen::DeckPicker;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                let index = SettingsTab::ALL
                    .iter()
                    .position(|tab| *tab == self.settings_tab)
                    .unwrap_or(0);
                self.settings_tab =
                    SettingsTab::ALL[index.saturating_sub(1)];
            }
            KeyCode::Right | KeyCode::Char('l') => {
                let index = SettingsTab::ALL
                    .iter()
                    .position(|tab| *tab == self.settings_tab)
                    .unwrap_or(0);
                self.settings_tab = SettingsTab::ALL[
                    (index + 1).min(SettingsTab::ALL.len() - 1)
                ];
            }
            KeyCode::Up | KeyCode::Char('k')
                if self.settings_tab == SettingsTab::Theme =>
            {
                self.selected_theme_preset =
                    self.selected_theme_preset.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j')
                if self.settings_tab == SettingsTab::Theme =>
            {
                if !self.theme_library.is_empty() {
                    self.selected_theme_preset =
                        (self.selected_theme_preset + 1)
                            .min(self.theme_library.len() - 1);
                }
            }
            KeyCode::Enter
                if self.settings_tab == SettingsTab::Theme =>
            {
                self.apply_selected_theme_preset();
            }
            KeyCode::Char('n') | KeyCode::Char('N')
                if self.settings_tab == SettingsTab::Theme =>
            {
                self.modal = Modal::ThemeName {
                    buffer: String::new(),
                };
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_modal_key(&mut self, key: KeyEvent) -> Result<()> {
        let modal = std::mem::replace(&mut self.modal, Modal::None);

        self.modal = match modal {
            Modal::CreateMenu { mut selected } => match key.code {
                KeyCode::Esc => Modal::None,
                KeyCode::Up => {
                    selected = selected.saturating_sub(1);
                    Modal::CreateMenu { selected }
                }
                KeyCode::Down => {
                    selected = (selected + 1).min(3);
                    Modal::CreateMenu { selected }
                }
                KeyCode::Enter => match selected {
                    0 => Modal::PasteBody {
                        buffer: String::new(),
                        scroll: 0,
                    },
                    1 => Modal::MoxfieldUrl {
                        buffer: String::new(),
                    },
                    2 => Modal::ArchidektUrl {
                        buffer: String::new(),
                    },
                    _ => Modal::GenerateCommander {
                        buffer: String::new(),
                    },
                },
                _ => Modal::CreateMenu { selected },
            },


            Modal::PasteBody {
                mut buffer,
                mut scroll,
            } => {
                let continue_import = key.code == KeyCode::F(2)
                    || (key.code == KeyCode::Enter
                        && key.modifiers.contains(KeyModifiers::CONTROL));

                if continue_import {
                    match parse_deck_text("Imported Deck", &buffer) {
                        Ok(deck) if deck.commanders.is_empty() => {
                            self.status =
                                "Commander was not detected. Tell Commander Lab who the commander is."
                                    .to_string();

                            self.commander_autocomplete_input.clear();
                            self.commander_autocomplete_results.clear();
                            self.commander_autocomplete_last_edit = None;
                            self.commander_autocomplete_ready = false;

                            Modal::PasteCommander {
                                deck,
                                buffer: String::new(),
                            }
                        }
                        Ok(deck) => Modal::PasteSaveName {
                            deck,
                            buffer: String::new(),
                        },
                        Err(err) => {
                            self.status =
                                format!("Could not parse pasted deck: {err:#}");

                            Modal::PasteBody {
                                buffer,
                                scroll,
                            }
                        }
                    }
                } else {
                    match key.code {
                        KeyCode::Esc => Modal::CreateMenu { selected: 0 },
                        KeyCode::PageUp | KeyCode::Up => {
                            scroll = scroll.saturating_sub(6);
                            Modal::PasteBody { buffer, scroll }
                        }
                        KeyCode::PageDown | KeyCode::Down => {
                            scroll = scroll.saturating_add(6);
                            Modal::PasteBody { buffer, scroll }
                        }
                        // Deliberately do not emulate an editor one character
                        // at a time for pasted decklists. Clipboard paste goes
                        // through Event::Paste and appends the complete payload
                        // in one operation.
                        KeyCode::Backspace => {
                            buffer.pop();
                            Modal::PasteBody { buffer, scroll }
                        }
                        KeyCode::Enter => {
                            buffer.push('\n');
                            scroll =
                                buffer.lines().count().saturating_sub(1);
                            Modal::PasteBody { buffer, scroll }
                        }
                        KeyCode::Char(ch) => {
                            buffer.push(ch);
                            scroll =
                                buffer.lines().count().saturating_sub(1);
                            Modal::PasteBody { buffer, scroll }
                        }
                        _ => Modal::PasteBody { buffer, scroll },
                    }
                }
            }

            Modal::PasteCommander {
                mut deck,
                mut buffer,
            } => match key.code {
                KeyCode::Esc => Modal::PasteBody {
                    buffer: deck_to_text(&deck),
                    scroll: 0,
                },
                KeyCode::Enter if !buffer.trim().is_empty() => {
                    match resolve_generation_commanders(
                        buffer.trim(),
                        &self.db,
                    ) {
                        Ok(cards) if !cards.is_empty() => {
                            let names = cards
                                .iter()
                                .map(|card| card.name.clone())
                                .collect::<Vec<_>>();

                            deck.commanders = names
                                .iter()
                                .map(|name| crate::deck::DeckEntry {
                                    quantity: 1,
                                    name: name.clone(),
                                })
                                .collect();

                            deck.main.retain(|entry| {
                                !names.iter().any(|name| {
                                    entry.name.eq_ignore_ascii_case(name)
                                })
                            });

                            self.status = format!(
                                "Commander set to {}. Name the deck or leave blank to use the commander name.",
                                names.join(" + "),
                            );

                            Modal::PasteSaveName {
                                deck,
                                buffer: String::new(),
                            }
                        }
                        Ok(_) => {
                            self.status =
                                "No commander could be resolved from that input."
                                    .to_string();
                            Modal::PasteCommander { deck, buffer }
                        }
                        Err(err) => {
                            self.status =
                                format!("Could not resolve commander: {err:#}");
                            Modal::PasteCommander { deck, buffer }
                        }
                    }
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    self.commander_autocomplete_ready = false;
                    self.commander_autocomplete_last_edit =
                        Some(Instant::now());
                    Modal::PasteCommander { deck, buffer }
                }
                KeyCode::Char(ch) => {
                    buffer.push(ch);
                    self.commander_autocomplete_ready = false;
                    self.commander_autocomplete_last_edit =
                        Some(Instant::now());
                    Modal::PasteCommander { deck, buffer }
                }
                _ => Modal::PasteCommander { deck, buffer },
            },

            Modal::PasteSaveName {
                mut deck,
                mut buffer,
            } => match key.code {
                KeyCode::Esc => {
                    if deck.commanders.is_empty() {
                        Modal::PasteCommander {
                            deck,
                            buffer: String::new(),
                        }
                    } else {
                        Modal::PasteBody {
                            buffer: deck_to_text(&deck),
                            scroll: 0,
                        }
                    }
                }
                KeyCode::Enter => {
                    let fallback = deck
                        .commanders
                        .iter()
                        .map(|entry| entry.name.clone())
                        .collect::<Vec<_>>()
                        .join(" + ");

                    let final_name = if buffer.trim().is_empty() {
                        if fallback.trim().is_empty() {
                            "Imported Deck".to_string()
                        } else {
                            fallback
                        }
                    } else {
                        buffer.trim().to_string()
                    };

                    deck.name = final_name.clone();
                    self.save_parsed_pasted_deck(&deck)?;
                    Modal::None
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    Modal::PasteSaveName { deck, buffer }
                }
                KeyCode::Char(ch) => {
                    buffer.push(ch);
                    Modal::PasteSaveName { deck, buffer }
                }
                _ => Modal::PasteSaveName { deck, buffer },
            }


            Modal::MoxfieldUrl { mut buffer } => match key.code {
                KeyCode::Esc => Modal::CreateMenu { selected: 1 },
                KeyCode::Enter if !buffer.trim().is_empty() => {
                    self.import_moxfield_deck(&buffer)?;
                    Modal::None
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    Modal::MoxfieldUrl { buffer }
                }
                KeyCode::Char(ch) => {
                    buffer.push(ch);
                    Modal::MoxfieldUrl { buffer }
                }
                _ => Modal::MoxfieldUrl { buffer },
            },

Modal::ArchidektUrl { mut buffer } => match key.code {
    KeyCode::Esc => Modal::CreateMenu { selected: 3 },
    KeyCode::Enter if !buffer.trim().is_empty() => {
        self.import_archidekt_deck(&buffer)?;
        Modal::None
    }
    KeyCode::Backspace => {
        buffer.pop();
        Modal::ArchidektUrl { buffer }
    }
    KeyCode::Char(ch) => {
        buffer.push(ch);
        Modal::ArchidektUrl { buffer }
    }
    _ => Modal::ArchidektUrl { buffer },
},

Modal::GenerateCommander { mut buffer } => match key.code {
    KeyCode::Esc => Modal::CreateMenu { selected: 3 },
    KeyCode::Enter if !buffer.trim().is_empty() => {
        match self.prepare_generation_themes(&buffer) {
            Ok((commander, themes)) => Modal::GenerateTheme {
                commander,
                themes,
                selected: 0,
            },
            Err(err) => {
                self.status = format!(
                    "Could not resolve commander: {err}"
                );
                Modal::GenerateCommander { buffer }
            }
        }
    }
    KeyCode::Backspace => {
        buffer.pop();
        self.hovered_commander_suggestion = None;
        self.commander_autocomplete_ready = false;
        self.commander_autocomplete_last_edit = Some(Instant::now());
        Modal::GenerateCommander { buffer }
    }
    KeyCode::Char(ch) => {
        buffer.push(ch);
        self.hovered_commander_suggestion = None;
        self.commander_autocomplete_ready = false;
        self.commander_autocomplete_last_edit = Some(Instant::now());
        Modal::GenerateCommander { buffer }
    }
    _ => Modal::GenerateCommander { buffer },
},

Modal::GenerateTheme {
    commander,
    themes,
    mut selected,
} => match key.code {
    KeyCode::Esc => Modal::GenerateCommander {
        buffer: commander,
    },
    KeyCode::Up => {
        selected = selected.saturating_sub(1);
        Modal::GenerateTheme {
            commander,
            themes,
            selected,
        }
    }
    KeyCode::Down => {
        if !themes.is_empty() {
            selected = (selected + 1).min(themes.len() - 1);
        }

        Modal::GenerateTheme {
            commander,
            themes,
            selected,
        }
    }
    KeyCode::Enter if !themes.is_empty() => {
        Modal::GenerateBracket {
            commander,
            theme: themes[selected].clone(),
            selected: 3,
        }
    }
    _ => Modal::GenerateTheme {
        commander,
        themes,
        selected,
    },
},

            Modal::GenerateBracket {
                commander,
                theme,
                mut selected,
            } => match key.code {
                KeyCode::Esc => Modal::GenerateTheme {
                    commander,
                    themes: vec![theme.clone()],
                    selected: 0,
                },
                KeyCode::Up | KeyCode::Left => {
                    selected = selected.saturating_sub(1).max(1);
                    Modal::GenerateBracket { commander, theme, selected }
                }
                KeyCode::Down | KeyCode::Right => {
                    selected = (selected + 1).min(5);
                    Modal::GenerateBracket { commander, theme, selected }
                }
                KeyCode::Char('1') => Modal::GenerateBracket {
                    commander,
                    theme,
                    selected: 1,
                },
                KeyCode::Char('2') => Modal::GenerateBracket {
                    commander,
                    theme,
                    selected: 2,
                },
                KeyCode::Char('3') => Modal::GenerateBracket {
                    commander,
                    theme,
                    selected: 3,
                },
                KeyCode::Char('4') => Modal::GenerateBracket {
                    commander,
                    theme,
                    selected: 4,
                },
                KeyCode::Char('5') => Modal::GenerateBracket {
                    commander,
                    theme,
                    selected: 5,
                },
                KeyCode::Enter => {
                    self.start_generation(&commander, &theme, selected)?;
                    Modal::GenerationProgress
                }
                _ => Modal::GenerateBracket { commander, theme, selected },
            },


Modal::GenerationProgress => match key.code {
    // Generation runs in a worker thread so the UI can continue
    // repainting. We intentionally do not abandon the worker on
    // Escape; the result will finish safely.
    KeyCode::Esc => {
        self.status =
            "Generation is still running; please wait for completion."
                .to_string();
        Modal::GenerationProgress
    }
    _ => Modal::GenerationProgress,
},

            Modal::DeleteDeckConfirm {
                deck_index,
                deck_name,
            } => match key.code {
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    Modal::None
                }
                KeyCode::Enter
                | KeyCode::Char('y')
                | KeyCode::Char('Y') => {
                    self.delete_deck(deck_index)?;
                    Modal::None
                }
                _ => Modal::DeleteDeckConfirm {
                    deck_index,
                    deck_name,
                },
            },


Modal::ThemePicker {
    slot,
    mut value,
    mut channel,
} => match key.code {
    KeyCode::Esc => Modal::None,
    KeyCode::Enter | KeyCode::Char('s') | KeyCode::Char('S') => {
        theme::set_slot(slot, value);

        match theme::save_theme("./data/theme.json") {
            Ok(_) => {
                self.status =
                    format!("{} saved as {}.", slot.label(), value.hex());
            }
            Err(err) => {
                self.status =
                    format!("Theme changed but save failed: {err}");
            }
        }

        Modal::None
    }
    KeyCode::Up => {
        channel = match channel {
            ThemeChannel::Red => ThemeChannel::Blue,
            ThemeChannel::Green => ThemeChannel::Red,
            ThemeChannel::Blue => ThemeChannel::Green,
        };

        Modal::ThemePicker { slot, value, channel }
    }
    KeyCode::Down | KeyCode::Tab => {
        channel = match channel {
            ThemeChannel::Red => ThemeChannel::Green,
            ThemeChannel::Green => ThemeChannel::Blue,
            ThemeChannel::Blue => ThemeChannel::Red,
        };

        Modal::ThemePicker { slot, value, channel }
    }
    KeyCode::Left => {
        let current = channel.value(value);
        channel.set(&mut value, current.saturating_sub(1));
        theme::set_slot(slot, value);
        Modal::ThemePicker { slot, value, channel }
    }
    KeyCode::Right => {
        let current = channel.value(value);
        channel.set(&mut value, current.saturating_add(1));
        theme::set_slot(slot, value);
        Modal::ThemePicker { slot, value, channel }
    }
    KeyCode::PageDown => {
        let current = channel.value(value);
        channel.set(&mut value, current.saturating_sub(16));
        theme::set_slot(slot, value);
        Modal::ThemePicker { slot, value, channel }
    }
    KeyCode::PageUp => {
        let current = channel.value(value);
        channel.set(&mut value, current.saturating_add(16));
        theme::set_slot(slot, value);
        Modal::ThemePicker { slot, value, channel }
    }
    KeyCode::Home => {
        channel.set(&mut value, 0);
        theme::set_slot(slot, value);
        Modal::ThemePicker { slot, value, channel }
    }
    KeyCode::End => {
        channel.set(&mut value, 255);
        theme::set_slot(slot, value);
        Modal::ThemePicker { slot, value, channel }
    }
    _ => Modal::ThemePicker { slot, value, channel },
},
            Modal::ThemeName { mut buffer } => match key.code {
                KeyCode::Esc => Modal::None,
                KeyCode::Enter if !buffer.trim().is_empty() => {
                    match theme::save_named_theme(
                        "./data/themes.json",
                        buffer.trim(),
                    ) {
                        Ok(_) => {
                            self.theme_library =
                                theme::load_named_themes("./data/themes.json");
                            self.selected_theme_preset = self
                                .theme_library
                                .iter()
                                .position(|item| {
                                    item.name.eq_ignore_ascii_case(
                                        buffer.trim()
                                    )
                                })
                                .unwrap_or(0);
                            let _ = theme::save_theme("./data/theme.json");
                            self.status = format!(
                                "Saved named theme '{}'.",
                                buffer.trim()
                            );
                        }
                        Err(err) => {
                            self.status =
                                format!("Could not save named theme: {err}");
                        }
                    }

                    Modal::None
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    Modal::ThemeName { buffer }
                }
                KeyCode::Char(ch) => {
                    buffer.push(ch);
                    Modal::ThemeName { buffer }
                }
                _ => Modal::ThemeName { buffer },
            },
Modal::UserReferenceUrl { mut buffer } => {
    match key.code {
        KeyCode::Esc => Modal::None,
        KeyCode::Backspace => {
            buffer.pop();
            Modal::UserReferenceUrl { buffer }
        }
        KeyCode::Char(ch) => {
            buffer.push(ch);
            Modal::UserReferenceUrl { buffer }
        }
        KeyCode::Enter => {
            let url = buffer.trim().to_string();

            if url.is_empty() {
                self.status =
                    "Paste or type a reference URL first."
                        .to_string();
                Modal::UserReferenceUrl { buffer }
            } else {
                match self.add_user_reference_url(&url) {
                    Ok(_) => Modal::None,
                    Err(err) => {
                        self.status = format!(
                            "Could not add reference: {err:#}"
                        );
                        Modal::UserReferenceUrl { buffer }
                    }
                }
            }
        }
        _ => Modal::UserReferenceUrl { buffer },
    }
}


Modal::AiModelPicker { mut selected } => {
    let config = AiConfig::load_or_create();
    let presets = model_presets(config.provider);

    match key.code {
        KeyCode::Esc => Modal::None,
        KeyCode::Up | KeyCode::Char('k') => {
            selected = selected.saturating_sub(1);
            Modal::AiModelPicker { selected }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if !presets.is_empty() {
                selected =
                    (selected + 1).min(presets.len() - 1);
            }
            Modal::AiModelPicker { selected }
        }
        KeyCode::Enter => {
            let mut config = config;

            if config.set_model_preset(selected) {
                let model = config.model.clone();
                self.save_ai_config_status(
                    &config,
                    format!(
                        "AI model set to {model}. Restart Commander Lab to load it.",
                    ),
                );
            }

            Modal::None
        }
        _ => Modal::AiModelPicker { selected },
    }
}

Modal::AiApiUrl { mut buffer } => match key.code {
    KeyCode::Esc => Modal::None,
    KeyCode::Enter => {
        let mut config = AiConfig::load_or_create();

        if !buffer.trim().is_empty() {
            config.api_base_url =
                buffer.trim().trim_end_matches('/').to_string();
            config.base_url = config.api_base_url.clone();
        }

        self.save_ai_config_status(
            &config,
            "AI API base URL saved.",
        );
        Modal::None
    }
    KeyCode::Backspace => {
        buffer.pop();
        Modal::AiApiUrl { buffer }
    }
    KeyCode::Char(ch) => {
        buffer.push(ch);
        Modal::AiApiUrl { buffer }
    }
    _ => Modal::AiApiUrl { buffer },
},

Modal::AiApiModel { mut buffer } => match key.code {
    KeyCode::Esc => Modal::None,
    KeyCode::Enter => {
        let mut config = AiConfig::load_or_create();

        if !buffer.trim().is_empty() {
            config.api_model = buffer.trim().to_string();
            config.model = config.api_model.clone();
        }

        self.save_ai_config_status(
            &config,
            "AI API model saved.",
        );
        Modal::None
    }
    KeyCode::Backspace => {
        buffer.pop();
        Modal::AiApiModel { buffer }
    }
    KeyCode::Char(ch) => {
        buffer.push(ch);
        Modal::AiApiModel { buffer }
    }
    _ => Modal::AiApiModel { buffer },
},

Modal::AiApiKey { mut buffer } => match key.code {
    KeyCode::Esc => Modal::None,
    KeyCode::Enter => {
        let mut config = AiConfig::load_or_create();

        config.api_key = buffer;

        self.save_ai_config_status(
            &config,
            "AI API key saved locally.",
        );
        Modal::None
    }
    KeyCode::Backspace => {
        buffer.pop();
        Modal::AiApiKey { buffer }
    }
    KeyCode::Char(ch) => {
        buffer.push(ch);
        Modal::AiApiKey { buffer }
    }
    _ => Modal::AiApiKey { buffer },
},

Modal::DeepLearnUpgradeBracket { selected } => {
    match key.code {
        KeyCode::Esc => Modal::None,
        KeyCode::Left | KeyCode::Up | KeyCode::Char('k') => {
            Modal::DeepLearnUpgradeBracket {
                selected: selected.saturating_sub(1).max(1),
            }
        }
        KeyCode::Right | KeyCode::Down | KeyCode::Char('j') => {
            Modal::DeepLearnUpgradeBracket {
                selected: (selected + 1).min(5),
            }
        }
        KeyCode::Char('1') => {
            self.choose_deep_learn_upgrade_bracket(1)?;
            Modal::GenerationProgress
        }
        KeyCode::Char('2') => {
            self.choose_deep_learn_upgrade_bracket(2)?;
            Modal::GenerationProgress
        }
        KeyCode::Char('3') => {
            self.choose_deep_learn_upgrade_bracket(3)?;
            Modal::GenerationProgress
        }
        KeyCode::Char('4') => {
            self.choose_deep_learn_upgrade_bracket(4)?;
            Modal::GenerationProgress
        }
        KeyCode::Char('5') => {
            self.choose_deep_learn_upgrade_bracket(5)?;
            Modal::GenerationProgress
        }
        KeyCode::Enter => {
            self.choose_deep_learn_upgrade_bracket(selected)?;
            Modal::GenerationProgress
        }
        _ => Modal::DeepLearnUpgradeBracket { selected },
    }
}

            Modal::None => Modal::None,
        };

        Ok(())
    }

    fn handle_modal_paste(&mut self, text: &str) {
        match &mut self.modal {
            Modal::MoxfieldUrl { buffer }
            | Modal::ArchidektUrl { buffer }
            | Modal::GenerateCommander { buffer }
            | Modal::ThemeName { buffer }
            | Modal::AiApiUrl { buffer }
            | Modal::AiApiModel { buffer }
            | Modal::AiApiKey { buffer } => buffer.push_str(text),

            Modal::PasteBody { buffer, scroll } => {
                buffer.push_str(text);
                *scroll =
                    buffer.lines().count().saturating_sub(1);
            }

            Modal::PasteCommander { buffer, .. } => {
                buffer.push_str(text);
                self.commander_autocomplete_ready = false;
                self.commander_autocomplete_last_edit =
                    Some(Instant::now());
            }

            Modal::PasteSaveName { buffer, .. } => {
                buffer.push_str(text);
            }


            Modal::UserReferenceUrl { buffer } => {
                buffer.push_str(text);
            }

            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        if let Modal::PasteBody { scroll, .. } = &mut self.modal {
            match mouse.kind {
                MouseEventKind::ScrollDown => {
                    *scroll = scroll.saturating_add(4);
                    return Ok(());
                }
                MouseEventKind::ScrollUp => {
                    *scroll = scroll.saturating_sub(4);
                    return Ok(());
                }
                _ => {}
            }
        }

        if matches!(mouse.kind, MouseEventKind::Moved)
            && matches!(
                self.modal,
                Modal::GenerateCommander { .. }
                    | Modal::PasteCommander { .. }
            )
        {
            self.hovered_commander_suggestion = None;

            for (name, area) in
                self.regions.generate_commander_suggestions.clone()
            {
                if point_in(area, mouse.column, mouse.row) {
                    self.hovered_commander_suggestion = Some(name);
                    break;
                }
            }

            return Ok(());
        }

        if matches!(mouse.kind, MouseEventKind::Moved)
            && !self.modal.active()
        {
            self.hovered_text = None;

            for (pane, index, area) in self.regions.text_rows.clone() {
                if point_in(area, mouse.column, mouse.row) {
                    self.hovered_text = Some((pane, index));
                    break;
                }
            }

            for (index, area) in self.regions.source_rows.clone() {
                if point_in(area, mouse.column, mouse.row) {
                    self.selected_source = index;
                    break;
                }
            }
        }

        if matches!(mouse.kind, MouseEventKind::Moved)
            && self.screen == Screen::DeepLearn
            && !self.modal.active()
        {
            for (_, name, area) in self.regions.deep_learn_cards.clone() {
                if point_in(area, mouse.column, mouse.row) {
                    self.select_card_by_name(&name);
                    return Ok(());
                }
            }

            for (name, area) in self.regions.named_cards.clone() {
                if point_in(area, mouse.column, mouse.row) {
                    self.select_card_by_name(&name);
                    return Ok(());
                }
            }
        }

        if matches!(mouse.kind, MouseEventKind::Moved)
            && self.screen == Screen::Dashboard
            && !self.modal.active()
        {
            for (name, area) in self.regions.named_cards.clone() {
                if point_in(area, mouse.column, mouse.row) {
                    self.select_card_by_name(&name);
                    return Ok(());
                }
            }
        }

        if matches!(mouse.kind, MouseEventKind::Moved)
            && self.screen == Screen::Dashboard
            && self.page == Page::Combos
            && !self.modal.active()
        {
            for (index, area) in self.regions.combo_rows.clone() {
                if point_in(area, mouse.column, mouse.row) {
                    self.combo_focus_index = index;
                    self.preview_focused_combo_card();
                    return Ok(());
                }
            }
        }

        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            match mouse.kind {
                MouseEventKind::ScrollDown => {
                    if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Agent
                        && self
                            .regions
                            .agent_prompt_box
                            .map(|area| {
                                point_in(
                                    area,
                                    mouse.column,
                                    mouse.row,
                                )
                            })
                            .unwrap_or(false)
                    {
                        self.deep_agent_prompt_scroll =
                            self.deep_agent_prompt_scroll.saturating_add(3);
                    } else if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Sources
                        && !self.modal.active()
                    {
                        self.source_scroll = self.source_scroll
                            .saturating_add(1)
                            .min(self.deck_sources.len().saturating_sub(1));
                    } else if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Compare
                        && !self.modal.active()
                    {
                        self.compare_scroll =
                            self.compare_scroll.saturating_add(3);
                    } else if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Agent
                        && !self.modal.active()
                    {
                        self.agent_page_scroll =
                            self.agent_page_scroll.saturating_add(3);
                    } else if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Trace
                        && !self.modal.active()
                    {
                        self.trace_scroll =
                            self.trace_scroll.saturating_add(3);
                    } else if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Chat
                        && !self.modal.active()
                    {
                        self.agent_chat_scroll =
                            self.agent_chat_scroll.saturating_sub(3);
                    } else if self.screen == Screen::Dashboard
                        && self.page == Page::Sources
                        && !self.modal.active()
                    {
                        self.source_scroll = self.source_scroll
                            .saturating_add(1)
                            .min(self.deck_sources.len().saturating_sub(1));
                    } else if self.screen == Screen::Dashboard
                        && self.page == Page::Log
                        && !self.modal.active()
                    {
                        self.deck_log_scroll =
                            self.deck_log_scroll.saturating_add(3);
                    } else if self.screen == Screen::DeckPicker && !self.modal.active() {
                        self.next_deck();
                    } else if self.page == Page::Cards && !self.modal.active() {
                        self.next_card();
                    }
                }
                MouseEventKind::ScrollUp => {
                    if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Agent
                        && self
                            .regions
                            .agent_prompt_box
                            .map(|area| {
                                point_in(
                                    area,
                                    mouse.column,
                                    mouse.row,
                                )
                            })
                            .unwrap_or(false)
                    {
                        self.deep_agent_prompt_scroll =
                            self.deep_agent_prompt_scroll.saturating_sub(3);
                    } else if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Sources
                        && !self.modal.active()
                    {
                        self.source_scroll =
                            self.source_scroll.saturating_sub(1);
                    } else if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Compare
                        && !self.modal.active()
                    {
                        self.compare_scroll =
                            self.compare_scroll.saturating_sub(3);
                    } else if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Agent
                        && !self.modal.active()
                    {
                        self.agent_page_scroll =
                            self.agent_page_scroll.saturating_sub(3);
                    } else if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Trace
                        && !self.modal.active()
                    {
                        self.trace_scroll =
                            self.trace_scroll.saturating_sub(3);
                    } else if self.screen == Screen::DeepLearn
                        && self.deep_learn_page == DeepLearnPage::Chat
                        && !self.modal.active()
                    {
                        self.agent_chat_scroll =
                            self.agent_chat_scroll.saturating_add(3);
                    } else if self.screen == Screen::Dashboard
                        && self.page == Page::Sources
                        && !self.modal.active()
                    {
                        self.source_scroll =
                            self.source_scroll.saturating_sub(1);
                    } else if self.screen == Screen::Dashboard
                        && self.page == Page::Log
                        && !self.modal.active()
                    {
                        self.deck_log_scroll =
                            self.deck_log_scroll.saturating_sub(3);
                    } else if self.screen == Screen::DeckPicker && !self.modal.active() {
                        self.prev_deck();
                    } else if self.page == Page::Cards && !self.modal.active() {
                        self.prev_card();
                    }
                }
                _ => {}
            }

            return Ok(());
        }

        if self.click_button(mouse.column, mouse.row)? {
            return Ok(());
        }

        if matches!(
            self.modal,
            Modal::GenerateCommander { .. }
                | Modal::PasteCommander { .. }
        ) {
            for (name, area) in
                self.regions.generate_commander_suggestions.clone()
            {
                if point_in(area, mouse.column, mouse.row) {
                    self.accept_commander_suggestion(&name)?;
                    return Ok(());
                }
            }
        }

        if self.modal.active() {
            return Ok(());
        }

        if self.screen == Screen::DeepLearn
            && self.deep_learn_page == DeepLearnPage::Agent
        {
            if let Some(area) = self.regions.agent_prompt_box {
                if point_in(area, mouse.column, mouse.row) {
                    if !self.is_ai_optimization_running() {
                        self.deep_agent_prompt_editing = true;
                        self.deep_agent_prompt_scroll = usize::MAX;
                    }
                    return Ok(());
                }
            }
        }

        for (slot, area) in self.regions.theme_slots.clone() {
            if point_in(area, mouse.column, mouse.row) {
                self.modal = Modal::ThemePicker {
                    slot,
                    value: theme::slot_value(slot),
                    channel: ThemeChannel::Red,
                };
                return Ok(());
            }
        }

        for (index, area) in self.regions.theme_presets.clone() {
            if point_in(area, mouse.column, mouse.row) {
                self.selected_theme_preset = index;
                self.apply_selected_theme_preset();
                return Ok(());
            }
        }

        match self.screen {
            Screen::Settings => {
                // Theme swatches and buttons are handled globally above.
            }

            Screen::DeckPicker => {
                for (index, area) in self.regions.deck_rows.clone() {
                    if point_in(area, mouse.column, mouse.row) {
                        self.selected_deck = index;
                        self.analyze_selected()?;
                        return Ok(());
                    }
                }

                // Compact fallback still uses the list region.
                if self.regions.deck_rows.is_empty() {
                    if let Some(area) = self.regions.deck_list {
                        if point_in(area, mouse.column, mouse.row) {
                            let inner_y =
                                mouse.row.saturating_sub(area.y + 1);
                            let idx = inner_y as usize;

                            if idx < self.decks.len() {
                                self.selected_deck = idx;
                                self.analyze_selected()?;
                            }
                        }
                    }
                }
            }

            Screen::DeepLearn => {
                for (page, area) in self.regions.deep_learn_tabs.clone() {
                    if point_in(area, mouse.column, mouse.row) {
                        self.deep_learn_page = page;
                        return Ok(());
                    }
                }

                for (index, name, area) in
                    self.regions.deep_learn_cards.clone()
                {
                    if point_in(area, mouse.column, mouse.row) {
                        self.deep_learn_selected = index;
                        self.select_card_by_name(&name);
                        return Ok(());
                    }
                }

                for (index, url, area) in
                    self.regions.source_links.clone()
                {
                    if point_in(area, mouse.column, mouse.row) {
                        self.selected_source = index;
                        self.open_url(&url, "source");
                        return Ok(());
                    }
                }

                for (name, area) in self.regions.named_cards.clone() {
                    if point_in(area, mouse.column, mouse.row) {
                        self.select_card_by_name(&name);
                        return Ok(());
                    }
                }
            }

            Screen::Dashboard => {
                for (page, area) in self.regions.tabs.clone() {
                    if point_in(area, mouse.column, mouse.row) {
                        self.page = page;
                        return Ok(());
                    }
                }

                for (tag, area) in self.regions.archetype_tags.clone() {
                    if point_in(area, mouse.column, mouse.row) {
                        self.select_archetype_tag(&tag);
                        return Ok(());
                    }
                }


                for (kind, name, area) in self.regions.suggestion_cards.clone() {
                    if point_in(area, mouse.column, mouse.row) {
                        self.selected_suggestion = Some(SuggestionSelection {
                            kind,
                            card: name.clone(),
                        });
                        self.select_card_by_name(&name);
                        self.status = match kind {
                            SuggestionKind::Cut => format!(
                                "Selected CUT {}. SIDEBOARD SELECTED moves it out of the main deck.",
                                name
                            ),
                            SuggestionKind::Add => format!(
                                "Selected ADD {}. SIDEBOARD SELECTED adds it to sideboard.",
                                name
                            ),
                        };
                        return Ok(());
                    }
                }

                for (index, url, area) in self.regions.source_links.clone() {
                    if point_in(area, mouse.column, mouse.row) {
                        self.selected_source = index;
                        self.open_url(&url, "source");
                        return Ok(());
                    }
                }

                for (index, name, area) in self.regions.deep_learn_cards.clone() {
                    if point_in(area, mouse.column, mouse.row) {
                        self.deep_learn_selected = index;
                        self.select_card_by_name(&name);
                        return Ok(());
                    }
                }

                for (index, area) in self.regions.combo_rows.clone() {
                    if point_in(area, mouse.column, mouse.row) {
                        self.combo_focus_index = index;
                        self.preview_focused_combo_card();
                        return Ok(());
                    }
                }

                for (name, area) in self.regions.named_cards.clone() {
                    if point_in(area, mouse.column, mouse.row) {
                        self.select_card_by_name(&name);
                        return Ok(());
                    }
                }

                if self.page == Page::Cards {
                    if let Some(area) = self.regions.card_list {
                        if point_in(area, mouse.column, mouse.row) {
                            let row =
                                mouse.row.saturating_sub(area.y + 1) as usize;
                            let idx = self.card_scroll + row;

                            if idx < self.card_count() {
                                self.selected_card = idx;
                                self.ensure_card_visible();
                                self.refresh_card_preview();
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn click_button(&mut self, x: u16, y: u16) -> Result<bool> {
        for (action, area) in self.regions.buttons.clone() {
            if !point_in(area, x, y) {
                continue;
            }

            match action {
                UiAction::Update => self.update_database(),
                UiAction::RefreshDecks => self.refresh_decks()?,
                UiAction::NewDeck => {
                    self.modal = Modal::CreateMenu { selected: 0 };
                }
                UiAction::DeleteDeck => self.open_delete_deck_modal(),
                UiAction::ConfirmDeleteDeck => {
                    if let Modal::DeleteDeckConfirm { deck_index, .. } =
                        self.modal.clone()
                    {
                        self.delete_deck(deck_index)?;
                        self.modal = Modal::None;
                    }
                }
                UiAction::Simulate => self.run_simulation()?,
                UiAction::Combos => self.run_combo_check()?,
                UiAction::Decks => self.screen = Screen::DeckPicker,
                UiAction::Quit => self.should_quit = true,
                UiAction::Mainboard => self.set_card_zone(CardZone::Mainboard),
                UiAction::Sideboard => self.set_card_zone(CardZone::Sideboard),
                UiAction::StoreTcgplayer => self.open_store(Store::Tcgplayer),
                UiAction::StoreCardmarket => self.open_store(Store::Cardmarket),
                UiAction::StoreCardhoarder => self.open_store(Store::Cardhoarder),
                UiAction::CreatePaste => {
                    self.modal = Modal::PasteBody {
                        buffer: String::new(),
                        scroll: 0,
                    };
                }
                UiAction::CreateMoxfield => {
                    self.modal = Modal::MoxfieldUrl {
                        buffer: String::new(),
                    };
                }
                UiAction::CreateArchidekt => {
                    self.modal = Modal::ArchidektUrl {
                        buffer: String::new(),
                    };
                }
                UiAction::CreateGenerate => {
                    self.modal = Modal::GenerateCommander {
                        buffer: String::new(),
                    };
                }
                UiAction::GenerateBracket1 => self.choose_generation_bracket(1)?,
                UiAction::GenerateBracket2 => self.choose_generation_bracket(2)?,
                UiAction::GenerateBracket3 => self.choose_generation_bracket(3)?,
                UiAction::GenerateBracket4 => self.choose_generation_bracket(4)?,
                UiAction::GenerateBracket5 => self.choose_generation_bracket(5)?,
                UiAction::GenerateTheme0 => self.choose_generation_theme(0),
                UiAction::GenerateTheme1 => self.choose_generation_theme(1),
                UiAction::GenerateTheme2 => self.choose_generation_theme(2),
                UiAction::GenerateTheme3 => self.choose_generation_theme(3),
                UiAction::GenerateTheme4 => self.choose_generation_theme(4),
                UiAction::GenerateTheme5 => self.choose_generation_theme(5),
                UiAction::GenerateTheme6 => self.choose_generation_theme(6),
                UiAction::GenerateTheme7 => self.choose_generation_theme(7),
                UiAction::GenerateTheme8 => self.choose_generation_theme(8),
                UiAction::GenerateTheme9 => self.choose_generation_theme(9),
                UiAction::GenerateTheme10 => self.choose_generation_theme(10),
                UiAction::GenerateTheme11 => self.choose_generation_theme(11),
                UiAction::SettingsOpen => {
                    self.screen = Screen::Settings;
                }
                UiAction::SettingsTabTheme => {
                    self.settings_tab = SettingsTab::Theme;
                }
                UiAction::SettingsTabAiModel => {
                    self.settings_tab = SettingsTab::AiModel;
                }
                UiAction::SettingsTabPricing => {
                    self.settings_tab = SettingsTab::Pricing;
                }
                UiAction::SettingsCycleAiProvider => {
                    let mut config = AiConfig::load_or_create();
                    config.cycle_provider();

                    match config.save() {
                        Ok(_) => {
                            self.status = format!(
                                "AI provider set to {}. Restart Commander Lab to apply the runtime change.",
                                config.provider.label(),
                            );
                        }
                        Err(err) => {
                            self.status = format!(
                                "AI provider changed to {} but could not save: {err}",
                                config.provider.label(),
                            );
                        }
                    }
                }
                UiAction::SettingsOpenModelPicker => {
                    let config = AiConfig::load_or_create();

                    if config.provider == crate::ai::AiProvider::Api {
                        self.modal = Modal::AiApiModel {
                            buffer: config.model,
                        };
                    } else {
                        let selected = model_presets(config.provider)
                            .iter()
                            .position(|preset| {
                                preset.model.eq_ignore_ascii_case(
                                    &config.model,
                                )
                            })
                            .unwrap_or(0);

                        self.modal = Modal::AiModelPicker { selected };
                    }
                }
                UiAction::SettingsAiModel0 => {
                    self.apply_ai_model_picker_choice(0);
                }
                UiAction::SettingsAiModel1 => {
                    self.apply_ai_model_picker_choice(1);
                }
                UiAction::SettingsAiModel2 => {
                    self.apply_ai_model_picker_choice(2);
                }
                UiAction::SettingsAiModel3 => {
                    self.apply_ai_model_picker_choice(3);
                }
                UiAction::SettingsEditApiUrl => {
                    let config = AiConfig::load_or_create();
                    self.modal = Modal::AiApiUrl {
                        buffer: config.base_url,
                    };
                }
                UiAction::SettingsEditApiKey => {
                    self.modal = Modal::AiApiKey {
                        buffer: String::new(),
                    };
                }
                UiAction::SettingsToggleWebResearch => {
                    let mut config = AiConfig::load_or_create();
                    config.web_research = !config.web_research;
                    let state = if config.web_research {
                        "ON"
                    } else {
                        "OFF"
                    };
                    self.save_ai_config_status(
                        &config,
                        format!("Web research set to {state}."),
                    );
                }
                UiAction::ThemeToggleTransparency => {
                    let transparent =
                        theme::toggle_transparent_background();

                    match theme::save_ui_style("./data/ui_style.json") {
                        Ok(_) => {
                            self.status = format!(
                                "Transparent background {}.",
                                if transparent {
                                    "enabled"
                                } else {
                                    "disabled"
                                },
                            );
                        }
                        Err(err) => {
                            self.status = format!(
                                "Transparency changed but could not save: {err}"
                            );
                        }
                    }
                }
                UiAction::ThemeSave => match theme::save_theme("./data/theme.json") {
                    Ok(_) => self.status =
                        "Theme saved to data/theme.json.".to_string(),
                    Err(err) => self.status =
                        format!("Could not save theme: {err}"),
                },
                UiAction::ThemeReset => {
                    theme::reset();
                    match theme::save_theme("./data/theme.json") {
                        Ok(_) => self.status =
                            "Theme reset and saved.".to_string(),
                        Err(err) => self.status =
                            format!("Theme reset but save failed: {err}"),
                    }
                }
                UiAction::ThemeToggleEdges => {
                    let style = theme::toggle_edge_style();
                    match theme::save_ui_style("./data/ui_style.json") {
                        Ok(_) => {
                            self.status = format!(
                                "UI edges changed to {} and saved.",
                                style.label(),
                            );
                        }
                        Err(err) => {
                            self.status = format!(
                                "UI edges changed to {} but save failed: {err}",
                                style.label(),
                            );
                        }
                    }
                }
                UiAction::SettingsCycleCurrency => {
                    let currency = theme::cycle_price_currency();
                    match theme::save_ui_style("./data/ui_style.json") {
                        Ok(_) => {
                            self.status = format!(
                                "Price currency changed to {} and saved.",
                                currency.label(),
                            );
                        }
                        Err(err) => {
                            self.status = format!(
                                "Price currency changed to {} but save failed: {err}",
                                currency.label(),
                            );
                        }
                    }
                }
                UiAction::SideboardSuggestion => {
                    self.sideboard_selected_suggestion()?;
                }
                UiAction::CopyDecklist => self.copy_current_deck_to_clipboard(),
                UiAction::RunDeepLearn => {
                    self.screen = Screen::DeepLearn;
                    self.deep_learn_page = DeepLearnPage::Overview;
                }
                UiAction::OpenSelectedCombo => self.open_selected_combo(),
                UiAction::ThemeSaveNamed => {
                    self.modal = Modal::ThemeName {
                        buffer: String::new(),
                    };
                }
                UiAction::AnalyzeSelected => {
                    self.analyze_selected()?;
                }
                UiAction::OpenCardsPage => {
                    self.page = Page::Cards;
                }
                UiAction::OpenBracketPage => {
                    self.page = Page::Bracket;
                }
                UiAction::OpenCombosPage => {
                    self.page = Page::Combos;
                    if self.combos.is_none() {
                        self.run_combo_check()?;
                    }
                }
                UiAction::OpenDeepLearnPage => {
                    self.screen = Screen::DeepLearn;
                    self.deep_learn_page = DeepLearnPage::Overview;
                }
                UiAction::OpenAgentChatPage => {
                    self.screen = Screen::DeepLearn;
                    self.open_agent_chat_tab();
                }
                UiAction::PickerDeepLearn => {
                    self.analyze_selected()?;
                    self.screen = Screen::DeepLearn;
                    self.deep_learn_page = DeepLearnPage::Overview;
                }
                UiAction::DeepLearnRunResearch => {
                    self.start_deep_learn()?;
                    self.deep_learn_page = DeepLearnPage::Research;
                }
                UiAction::DeepLearnRunSimulation => {
                    self.deep_learn_intensity =
                        TestingIntensity::Thorough;
                    self.start_deep_learn_simulation()?;
                    self.deep_learn_page = DeepLearnPage::Simulate;
                }
                UiAction::DeepAgentRun => {
                    self.open_agent_chat_tab();
                }
                UiAction::DeepAgentSaveBest => {
                    if let Some(result) = self.deep_agent_result.clone() {
                        self.finish_generated_deck(result.best)?;
                        self.screen = Screen::DeepLearn;
                        self.deep_learn_page = DeepLearnPage::Agent;
                    } else {
                        self.status =
                            "Run the AI Optimization first.".to_string();
                    }
                }
                UiAction::DeepAgentChatSend => {
                    self.open_agent_chat_tab();
                    self.handle_agent_chat_submit()?;
                }
                UiAction::DeepAgentChatClear => {
                    self.open_agent_chat_tab();
                    self.clear_agent_chat();
                }
                UiAction::PasteDeckContinue => {
                    self.continue_pasted_deck_from_modal()?;
                }
                UiAction::PasteCommanderUse => {
                    self.accept_paste_commander_from_modal()?;
                }
                UiAction::GenerateCommanderUse => {
                    self.accept_generation_commander_from_modal()?;
                }
                UiAction::AddUserReference => {
                    self.modal = Modal::UserReferenceUrl {
                        buffer: String::new(),
                    };
                    self.status =
                        "Paste a Moxfield, Archidekt, primer, or other public reference URL."
                            .to_string();
                }
                UiAction::RemoveUserReference => {
                    self.remove_selected_user_reference()?;
                }
                UiAction::DeepLearnCopyDeck => {
                    self.copy_current_deck_to_clipboard();
                }
                UiAction::DeepLearnUpgrade => {
                    self.modal = Modal::DeepLearnUpgradeBracket {
                        selected: self.deep_learn_upgrade_bracket,
                    };
                }
                UiAction::DeepLearnUpgrade1 => {
                    self.choose_deep_learn_upgrade_bracket(1)?;
                }
                UiAction::DeepLearnUpgrade2 => {
                    self.choose_deep_learn_upgrade_bracket(2)?;
                }
                UiAction::DeepLearnUpgrade3 => {
                    self.choose_deep_learn_upgrade_bracket(3)?;
                }
                UiAction::DeepLearnUpgrade4 => {
                    self.choose_deep_learn_upgrade_bracket(4)?;
                }
                UiAction::DeepLearnUpgrade5 => {
                    self.choose_deep_learn_upgrade_bracket(5)?;
                }
                UiAction::ModalCancel => self.modal = Modal::None,
            }

            return Ok(true);
        }

        Ok(false)
    }

    fn next_deck(&mut self) {
        if self.decks.is_empty() {
            return;
        }

        self.selected_deck = (self.selected_deck + 1) % self.decks.len();
    }

    fn prev_deck(&mut self) {
        if self.decks.is_empty() {
            return;
        }

        self.selected_deck = if self.selected_deck == 0 {
            self.decks.len() - 1
        } else {
            self.selected_deck - 1
        };
    }

    fn refresh_decks(&mut self) -> Result<()> {
        self.decks = load_deck_files("./decks")?;

        if self.selected_deck >= self.decks.len() {
            self.selected_deck = 0;
        }

        self.status = format!("Refreshed deck library: {} file(s).", self.decks.len());
        Ok(())
    }

    fn continue_pasted_deck_from_modal(&mut self) -> Result<()> {
        let Modal::PasteBody { buffer, scroll } =
            self.modal.clone()
        else {
            return Ok(());
        };

        self.modal = match parse_deck_text("Imported Deck", &buffer) {
            Ok(deck) if deck.commanders.is_empty() => {
                self.commander_autocomplete_input.clear();
                self.commander_autocomplete_results.clear();
                self.commander_autocomplete_last_edit = None;
                self.commander_autocomplete_ready = false;
                self.hovered_commander_suggestion = None;

                Modal::PasteCommander {
                    deck,
                    buffer: String::new(),
                }
            }
            Ok(deck) => Modal::PasteSaveName {
                deck,
                buffer: String::new(),
            },
            Err(err) => {
                self.status =
                    format!("Could not parse pasted deck: {err:#}");
                Modal::PasteBody { buffer, scroll }
            }
        };

        Ok(())
    }

    fn resolve_paste_commander_modal(
        &mut self,
        mut deck: ParsedDeck,
        buffer: String,
    ) -> Modal {
        if buffer.trim().is_empty() {
            self.status = "Enter a commander name.".to_string();
            return Modal::PasteCommander { deck, buffer };
        }

        match resolve_generation_commanders(buffer.trim(), &self.db) {
            Ok(cards) if !cards.is_empty() => {
                let names = cards
                    .iter()
                    .map(|card| card.name.clone())
                    .collect::<Vec<_>>();

                deck.commanders = names
                    .iter()
                    .map(|name| crate::deck::DeckEntry {
                        quantity: 1,
                        name: name.clone(),
                    })
                    .collect();

                deck.main.retain(|entry| {
                    !names.iter().any(|name| {
                        entry.name.eq_ignore_ascii_case(name)
                    })
                });

                self.hovered_commander_suggestion = None;
                self.commander_autocomplete_results.clear();
                self.status = format!("Commander: {}", names.join(" + "));

                Modal::PasteSaveName {
                    deck,
                    buffer: String::new(),
                }
            }
            Ok(_) => {
                self.status = "Commander could not be resolved.".to_string();
                Modal::PasteCommander { deck, buffer }
            }
            Err(err) => {
                self.status = format!("Could not resolve commander: {err:#}");
                Modal::PasteCommander { deck, buffer }
            }
        }
    }

    fn accept_paste_commander_from_modal(&mut self) -> Result<()> {
        let Modal::PasteCommander { deck, buffer } =
            self.modal.clone()
        else {
            return Ok(());
        };

        self.modal = self.resolve_paste_commander_modal(deck, buffer);
        Ok(())
    }

    fn accept_generation_commander_from_modal(
        &mut self,
    ) -> Result<()> {
        let Modal::GenerateCommander { buffer } =
            self.modal.clone()
        else {
            return Ok(());
        };

        if buffer.trim().is_empty() {
            self.status = "Enter a commander name.".to_string();
            return Ok(());
        }

        match self.prepare_generation_themes(&buffer) {
            Ok((commander, themes)) => {
                self.modal = Modal::GenerateTheme {
                    commander,
                    themes,
                    selected: 0,
                };
            }
            Err(err) => {
                self.status =
                    format!("Could not resolve commander: {err}");
            }
        }

        Ok(())
    }

    fn replace_last_commander_segment(
        buffer: &str,
        selected_name: &str,
    ) -> String {
        let separators = ['+', '&', '|'];
        let last_separator = buffer
            .char_indices()
            .rev()
            .find(|(_, ch)| separators.contains(ch))
            .map(|(index, ch)| (index, ch.len_utf8()));

        if let Some((index, separator_len)) = last_separator {
            let prefix = &buffer[..index + separator_len];
            format!("{} {}", prefix.trim_end(), selected_name)
        } else {
            selected_name.to_string()
        }
    }

    fn accept_commander_suggestion(
        &mut self,
        selected_name: &str,
    ) -> Result<()> {
        match self.modal.clone() {
            Modal::GenerateCommander { buffer } => {
                let input = Self::replace_last_commander_segment(
                    &buffer,
                    selected_name,
                );

                match self.prepare_generation_themes(&input) {
                    Ok((commander, themes)) => {
                        self.modal = Modal::GenerateTheme {
                            commander,
                            themes,
                            selected: 0,
                        };
                    }
                    Err(err) => {
                        self.status =
                            format!("Could not resolve commander: {err}");
                        self.modal =
                            Modal::GenerateCommander { buffer: input };
                    }
                }
            }
            Modal::PasteCommander { deck, buffer } => {
                let input = Self::replace_last_commander_segment(
                    &buffer,
                    selected_name,
                );
                self.modal =
                    self.resolve_paste_commander_modal(deck, input);
            }
            _ => {}
        }

        self.hovered_commander_suggestion = None;
        self.commander_autocomplete_input.clear();
        self.commander_autocomplete_results.clear();
        self.commander_autocomplete_last_edit = None;
        self.commander_autocomplete_ready = false;

        Ok(())
    }

    fn save_parsed_pasted_deck(
        &mut self,
        deck: &ParsedDeck,
    ) -> Result<()> {
        let file_name = format!(
            "{}.txt",
            safe_deck_filename(&deck.name),
        );
        let path = PathBuf::from("./decks").join(file_name);

        write_deck_file(&path, deck)?;
        self.refresh_decks()?;
        self.status =
            format!("Saved pasted deck as {}.", path.display());

        Ok(())
    }

    fn import_moxfield_deck(&mut self, url: &str) -> Result<()> {
        match import_moxfield(url) {
            Ok(deck) => {
                let file_name =
                    format!("{}.txt", safe_deck_filename(&deck.name));
                let path = PathBuf::from("./decks").join(file_name);
                write_deck_file(&path, &deck)?;
                self.refresh_decks()?;
                self.status =
                    format!("Imported Moxfield deck as {}.", path.display());
            }

            Err(err) => {
                self.status = format!("Moxfield import failed: {err}");
            }
        }

        Ok(())
    }


fn import_archidekt_deck(&mut self, url: &str) -> Result<()> {
    match import_archidekt(url.trim()) {
        Ok(deck) => {
            let path = PathBuf::from("./decks")
                .join(format!("{}.txt", safe_deck_filename(&deck.name)));

            write_deck_file(&path, &deck)?;
            self.refresh_decks()?;

            self.status = format!(
                "Imported Archidekt deck '{}' with {} main/commander cards and {} sideboard cards.",
                deck.name,
                deck.commanders
                    .iter()
                    .chain(deck.main.iter())
                    .map(|entry| entry.quantity)
                    .sum::<u32>(),
                deck.sideboard.iter().map(|entry| entry.quantity).sum::<u32>()
            );
        }

        Err(err) => {
            self.status = format!("Archidekt import failed: {err}");
        }
    }

    Ok(())
}

fn copy_current_deck_to_clipboard(&mut self) {
    let Some(deck) = self.parsed.as_ref() else {
        self.status = "No analyzed deck is available to copy.".to_string();
        return;
    };

    let text = deck_to_text(deck);

    match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(text)) {
        Ok(_) => {
            self.status =
                "Exported Commander decklist to the system clipboard."
                    .to_string();
        }
        Err(err) => {
            self.status =
                format!("Could not copy decklist to clipboard: {err}");
        }
    }
}

fn prepare_generation_themes(
    &mut self,
    input: &str,
) -> Result<(String, Vec<String>)> {
    let commanders = resolve_generation_commanders(input, &self.db)?;

    let canonical = commanders
        .iter()
        .map(|card| card.name.clone())
        .collect::<Vec<_>>()
        .join(" + ");

    let themes = suggest_generation_themes(&commanders, &self.db)?
        .into_iter()
        .map(|theme| theme.name)
        .collect::<Vec<_>>();

    if canonical.eq_ignore_ascii_case(input.trim()) {
        self.status = format!(
            "Commander resolved: {canonical}. Choose the deck theme."
        );
    } else {
        self.status = format!(
            "Assuming you meant: {canonical}. Choose the deck theme."
        );
    }

    Ok((canonical, themes))
}

pub fn commander_autocomplete(
    &mut self,
    input: &str,
) -> (Vec<String>, bool) {
    let first = input
        .split(['+', '&', '|'])
        .last()
        .unwrap_or(input)
        .trim()
        .to_string();

    if first.is_empty() {
        self.commander_autocomplete_input.clear();
        self.commander_autocomplete_results.clear();
        self.commander_autocomplete_last_edit = None;
        self.commander_autocomplete_ready = false;
        return (Vec::new(), false);
    }

    // Rendering happens every ~50 ms. Previously this function performed the
    // SQLite + fuzzy ranking pass on every frame while the modal was open,
    // which made typing feel sticky. A changed query now only starts/restarts
    // the debounce timer.
    if !self
        .commander_autocomplete_input
        .eq_ignore_ascii_case(&first)
    {
        self.commander_autocomplete_input = first.clone();
        self.commander_autocomplete_results.clear();
        self.commander_autocomplete_last_edit = Some(Instant::now());
        self.commander_autocomplete_ready = false;

        return (Vec::new(), true);
    }

    if self.commander_autocomplete_ready {
        return (
            self.commander_autocomplete_results.clone(),
            false,
        );
    }

    let elapsed = self
        .commander_autocomplete_last_edit
        .map(|instant| instant.elapsed())
        .unwrap_or_default();

    if elapsed
        < Duration::from_millis(
            COMMANDER_AUTOCOMPLETE_DEBOUNCE_MS,
        )
    {
        return (Vec::new(), true);
    }

    // Avoid doing a broad fuzzy database scan for a single character. The
    // user can keep typing and the search will begin once there is enough
    // information to produce useful matches.
    if first.chars().count() < 2 {
        return (Vec::new(), true);
    }

    let results = commander_name_suggestions(
        &first,
        &self.db,
        5,
    )
    .unwrap_or_default()
    .into_iter()
    .map(|card| card.name)
    .collect::<Vec<_>>();

    self.commander_autocomplete_results = results.clone();
    self.commander_autocomplete_ready = true;

    (
        results,
        false,
    )
}


fn choose_generation_theme(&mut self, index: usize) {
    let Modal::GenerateTheme {
        commander,
        themes,
        ..
    } = self.modal.clone()
    else {
        return;
    };

    if let Some(theme) = themes.get(index) {
        self.modal = Modal::GenerateBracket {
            commander,
            theme: theme.clone(),
            selected: 3,
        };
    }
}

fn choose_generation_bracket(&mut self, bracket: u8) -> Result<()> {
    let Modal::GenerateBracket {
        commander,
        theme,
        ..
    } = self.modal.clone()
    else {
        return Ok(());
    };

    self.start_generation(&commander, &theme, bracket)?;
    self.modal = Modal::GenerationProgress;
    Ok(())
}

fn choose_deep_learn_upgrade_bracket(
    &mut self,
    bracket: u8,
) -> Result<()> {
    self.deep_learn_upgrade_bracket = bracket.clamp(1, 5);
    self.start_deep_learn_upgrade(self.deep_learn_upgrade_bracket)?;
    self.modal = Modal::GenerationProgress;
    Ok(())
}

fn start_deep_learn_upgrade(
    &mut self,
    bracket: u8,
) -> Result<()> {
    if self.generation_rx.is_some() {
        self.status =
            "A generation/upgrade job is already running.".to_string();
        return Ok(());
    }

    let Some(current) = self.parsed.clone() else {
        self.status =
            "Analyze a deck before running AI Upgrade.".to_string();
        return Ok(());
    };

    let (tx, rx) = mpsc::channel::<GenerationEvent>();

    self.generation_trace.clear();
    push_ui_trace(
        &mut self.generation_trace,
        "GEN",
        format!(
            "AI UPGRADE START • deck={} • requested_bracket=B{} • commanders={}",
            current.name,
            bracket,
            current.commanders
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>()
                .join(" + "),
        ),
    );

    self.generation_progress = Some(GenerationProgress {
        percent: 1,
        step: "Starting AI Upgrade".to_string(),
        detail: format!(
            "{} • Bracket {}",
            current.name,
            bracket,
        ),
    });

    self.generation_rx = Some(rx);

    thread::spawn(move || {
        let result = (|| -> Result<GeneratedDeck> {
            let db = CardDb::open("./data/cards.sqlite")?;

            generate_ai_upgrade(
                &current,
                GenerationBracket::new(bracket),
                None,
                &[],
                &db,
                |update| {
                    let _ =
                        tx.send(GenerationEvent::Progress(update));
                },
            )
        })();

        let _ = tx.send(GenerationEvent::Finished(
            result.map_err(|err| format!("{err:#}")),
        ));
    });

    Ok(())
}

fn start_generation(
    &mut self,
    commander: &str,
    theme: &str,
    bracket: u8,
) -> Result<()> {
    if self.generation_rx.is_some() {
        self.status = "A deck generation job is already running.".to_string();
        return Ok(());
    }

    let commander = commander.to_string();
    let theme = theme.to_string();
    let (tx, rx) = mpsc::channel::<GenerationEvent>();

    self.generation_trace.clear();
    push_ui_trace(&mut self.generation_trace, "GEN", format!(
        "START • commander={} • theme={} • requested_bracket=B{}",
        commander,
        theme,
        bracket,
    ));

    self.generation_progress = Some(GenerationProgress {
        percent: 1,
        step: "Starting generation".to_string(),
        detail: format!(
            "{} • {} • Bracket {}",
            commander,
            theme,
            bracket
        ),
    });
    self.generation_rx = Some(rx);

    thread::spawn(move || {
        let result = (|| -> Result<GeneratedDeck> {
            let db = CardDb::open("./data/cards.sqlite")?;

            generate_optimized_deck(
                &commander,
                &theme,
                GenerationBracket::new(bracket),
                &db,
                |update| {
                    let _ = tx.send(GenerationEvent::Progress(update));
                },
            )
        })();

        let finished = result.map_err(|err| format!("{err:#}"));
        let _ = tx.send(GenerationEvent::Finished(finished));
    });

    Ok(())
}

fn poll_generation_events(&mut self) -> Result<()> {
    let mut finished = None;

    if let Some(rx) = &self.generation_rx {
        loop {
            match rx.try_recv() {
                Ok(GenerationEvent::Progress(update)) => {
                    self.status = format!(
                        "{}% • {} • {}",
                        update.percent,
                        update.step,
                        update.detail
                    );
                    push_ui_trace(&mut self.generation_trace, "GEN", format!(
                        "{:>3}% • {} • {}",
                        update.percent,
                        update.step,
                        update.detail,
                    ));

                    self.generation_progress = Some(update);
                }

                Ok(GenerationEvent::Finished(result)) => {
                    push_ui_trace(&mut self.generation_trace, "GEN", 
                        "WORKER • generation task returned",
                    );
                    finished = Some(result);
                    break;
                }

                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    push_ui_trace(&mut self.generation_trace, "GEN", 
                        "ERROR • generation worker disconnected unexpectedly",
                    );
                    finished = Some(Err(
                        "Generation worker disconnected unexpectedly."
                            .to_string(),
                    ));
                    break;
                }
            }
        }
    }

    if let Some(result) = finished {
        self.generation_rx = None;

        match result {
            Ok(generated) => {
                push_ui_trace(&mut self.generation_trace, "GEN", format!(
                    "SUCCESS • generated={} • combos={} • sources={}",
                    generated.deck.name,
                    generated.plan.combos.len(),
                    generated.plan.sources.len(),
                ));

                for combo in &generated.plan.combos {
                    push_ui_trace(
                        &mut self.generation_trace,
                        "GEN",
                        format!(
                            "COMBO • {} • result={} • source={}",
                            combo.cards.join(" + "),
                            combo.result,
                            combo.source,
                        ),
                    );
                }

                for note in generated.plan.notes.iter().take(16) {
                    push_ui_trace(
                        &mut self.generation_trace,
                        "GEN",
                        format!("PLAN • {note}"),
                    );
                }

                self.finish_generated_deck(generated)?;
            }
            Err(err) => {
                push_ui_trace(&mut self.generation_trace, "GEN", format!(
                    "FAILED • {err}",
                ));
                self.status = format!("Deck generation failed: {err}");
                self.generation_progress = None;
                self.modal = Modal::None;
            }
        }
    }

    Ok(())
}

fn finish_generated_deck(
    &mut self,
    generated: GeneratedDeck,
) -> Result<()> {
    let file_name =
        format!("{}.txt", safe_deck_filename(&generated.deck.name));
    let path = PathBuf::from("./decks").join(file_name);

    write_deck_file(&path, &generated.deck)?;

    let plan_path = PathBuf::from("./decks").join(format!(
        "{}.plan.md",
        safe_deck_filename(&generated.deck.name)
    ));

    std::fs::write(
        &plan_path,
        game_plan_markdown(&generated.plan),
    )?;

    save_sources_for_deck(
        &path,
        &generated.plan.sources,
    )?;

        self.refresh_decks()?;
    self.generation_progress = None;
    self.modal = Modal::None;

    self.status = format!(
        "Generated {} • theme {} • {} combo line(s) • saved deck + game plan.",
        generated.plan.commanders.join(" + "),
        generated
            .plan
            .primary_tags
            .first()
            .cloned()
            .unwrap_or_else(|| "synergy".to_string()),
        generated.plan.combos.len(),
    );

    Ok(())
}


fn open_delete_deck_modal(&mut self) {
    let Some(deck) = self.decks.get(self.selected_deck) else {
        self.status = "No deck selected to delete.".to_string();
        return;
    };

    self.modal = Modal::DeleteDeckConfirm {
        deck_index: self.selected_deck,
        deck_name: deck.name.clone(),
    };
}

fn delete_deck(&mut self, deck_index: usize) -> Result<()> {
    let Some(deck) = self.decks.get(deck_index).cloned() else {
        self.status = "Deck no longer exists in the library.".to_string();
        return Ok(());
    };

    if deck.path.exists() {
        std::fs::remove_file(&deck.path)?;
    }

    delete_sources_sidecar(&deck.path);
    delete_history(&deck.path);

    let plan_path = deck.path.with_extension("plan.md");
    if plan_path.exists() {
        let _ = std::fs::remove_file(&plan_path);
    }

    // Older generated-plan naming may use `<stem>.plan.md`.
    if let Some(stem) = deck.path.file_stem().and_then(|value| value.to_str()) {
        let alternate_plan = deck
            .path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("./decks"))
            .join(format!("{stem}.plan.md"));

        if alternate_plan.exists() {
            let _ = std::fs::remove_file(alternate_plan);
        }
    }

    let deleted_name = deck.name;
    self.refresh_decks()?;

    if self.decks.is_empty() {
        self.selected_deck = 0;
    } else if self.selected_deck >= self.decks.len() {
        self.selected_deck = self.decks.len() - 1;
    }

    self.parsed = None;
    self.analysis = None;
    self.simulation = None;
    self.combos = None;
    self.selected_card_info = None;
    self.card_image = None;

    self.status = format!("Deleted deck '{deleted_name}'.");
    Ok(())
}


fn analyze_selected(&mut self) -> Result<()> {
        if !self.db.has_cards() {
            self.status =
                "No local card data yet. Click UPDATE or press U first.".to_string();
            return Ok(());
        }

        let Some(file) = self.decks.get(self.selected_deck) else {
            self.status = "No deck selected.".to_string();
            return Ok(());
        };

        let parsed = parse_deck_file(&file.path)?;
        let analysis = analyze_deck(&parsed, &self.db)?;

        self.parsed = Some(parsed);
        self.analysis = Some(analysis);
        self.simulation = None;
        self.combos = None;
        self.selected_archetype_tag = None;
        self.selected_suggestion = None;
        self.suggestion_focus_column = 0;
        self.suggestion_focus_index = 0;
        self.combo_focus_index = 0;
        self.simulation_focus_index = 0;
        self.selected_source = 0;
        self.source_scroll = 0;
        self.trace_scroll = 0;
        self.agent_page_scroll = 0;
        self.agent_chat_scroll = 0;
        self.deck_log_scroll = 0;
        self.deck_history_lines = render_history_lines(&file.path);
        let cached_history = load_history(&file.path);
        self.deep_learn = None;
        self.deep_learn_testing = None;
        self.deep_agent_result = None;
        self.deep_agent_comparison = None;
        self.compare_scroll = 0;
        self.deep_agent_trace.clear();
        self.deep_agent_progress = 0;
        self.deep_agent_current_task =
            "Waiting for Agent brief.".to_string();
        self.deep_agent_completed_tasks.clear();
        self.deep_agent_style_brief.clear();
        self.agent_chat_messages.clear();
        self.agent_chat_input.clear();
        self.agent_chat_status =
            "Chat with the Agent to refine the deck. Say \"run optimization\" to apply changes or ask for the decklist anytime."
                .to_string();
        self.deep_learn_page = DeepLearnPage::Overview;
        self.deep_learn_selected = 0;
        self.deep_learn_trace.clear();
        self.generation_trace.clear();
        self.deep_learn_status =
            "AI Agent has not run for this deck yet.".to_string();

        let commander_cards = self
            .parsed
            .as_ref()
            .map(|deck| {
                deck.commanders
                    .iter()
                    .filter_map(|entry| self.db.get_card(&entry.name).ok().flatten())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        self.deck_sources = load_sources_for_deck(&file.path)?
            .unwrap_or_else(|| {
                sources_for_existing_deck(
                    self.parsed.as_ref().expect("parsed deck"),
                    &commander_cards,
                    self.analysis
                        .as_ref()
                        .map(|analysis| analysis.bracket.bracket)
                        .unwrap_or(3),
                )
            });

        self.agent_chat_messages = cached_history.agent_chat;
        self.agent_chat_input.clear();
        self.agent_chat_optimize_after_research = false;
        self.deep_agent_last_brief =
            cached_history.agent_last_brief;

        self.meta_matchup =
            cached_history.latest_meta_matchup.clone();

        if let Some(agent) = cached_history.latest_agent {
            self.sync_agent_workspace(&agent)?;
            self.deep_agent_trace = agent.trace.clone();
            self.deep_agent_progress = 100;
            self.deep_agent_current_task =
                "Cached optimization loaded".to_string();
            self.deep_agent_completed_tasks = agent
                .rounds
                .iter()
                .map(|round| {
                    format!(
                        "DONE • Round {} • {}",
                        round.round,
                        round.summary,
                    )
                })
                .collect();
            self.deep_learn_status = format!(
                "Cached AI Optimization • B{} • {} rounds • {} final games.",
                agent.target_bracket,
                agent.rounds.len(),
                agent.final_testing.goldfish.runs,
            );
            self.agent_chat_status =
                "Previous AI Agent session restored. Continue chatting or ask for another optimization/research pass."
                    .to_string();
            self.deep_agent_result = Some(agent);
        } else if let Some(report) =
            cached_history.latest_research
        {
            self.deep_learn_testing = report.testing.clone();
            self.deep_learn_trace =
                report.decision_trace.clone();
            self.deep_learn_status = format!(
                "Cached research loaded • {} references • {}% confidence.",
                report.reference_decks,
                report.confidence,
            );
            self.agent_chat_status =
                "Previous AI Agent research/chat restored."
                    .to_string();
            self.deep_learn = Some(report);
        } else if !self.agent_chat_messages.is_empty() {
            self.agent_chat_status =
                "Previous AI Agent chat restored."
                    .to_string();
        }

        self.page = Page::Overview;
        self.screen = Screen::Dashboard;
        self.card_zone = CardZone::Mainboard;
        self.selected_card = 0;
        self.card_scroll = 0;
        self.card_image = None;
        self.selected_card_info = None;
        self.refresh_card_preview();

        self.status =
            if self.deep_agent_result.is_some()
                || self.deep_learn.is_some()
                || !self.agent_chat_messages.is_empty()
            {
                "Local analysis complete • cached AI Agent session restored."
                    .to_string()
            } else {
                "Local analysis complete. Click tabs to explore."
                    .to_string()
            };
        Ok(())
    }

    fn reanalyze(&mut self) {
        if let Some(parsed) = self.parsed.clone() {
            match analyze_deck(&parsed, &self.db) {
                Ok(mut analysis) => {
                    if let Some(report) = &self.combos {
                        refine_with_combo_report(&mut analysis.bracket, report);
                    }

                    self.analysis = Some(analysis);
                    self.status = "Deck analysis refreshed.".to_string();
                }
                Err(err) => self.status = format!("Analysis failed: {err}"),
            }
        }
    }

    fn update_database(&mut self) {
        self.status =
            "Downloading Scryfall JSONL.GZ and rebuilding local SQLite database..."
                .to_string();

        match self.db.update_from_scryfall() {
            Ok(count) => {
                self.status =
                    format!("Scryfall update complete: imported {count} Oracle cards.");
                self.reanalyze();
                self.refresh_card_preview();
            }

            Err(err) => {
                self.status = format!("Scryfall update failed: {err}");
            }
        }
    }

    fn refresh_deck_history(&mut self) {
        if let Some(file) = self.decks.get(self.selected_deck) {
            self.deck_history_lines =
                render_history_lines(&file.path);
        }
    }

    fn cache_deck_run(
        &mut self,
        kind: &str,
        summary: String,
        log: Vec<String>,
    ) {
        if let Some(file) = self.decks.get(self.selected_deck) {
            if let Err(err) =
                append_deck_run(&file.path, kind, summary, log)
            {
                logging::record(
                    "DECK-HISTORY",
                    format!("WRITE FAILED • {err:#}"),
                );
            } else {
                self.refresh_deck_history();
            }
        }
    }

    fn persist_ai_session(&self) {
        let Some(file) = self.decks.get(self.selected_deck) else {
            return;
        };

        if let Err(err) = save_agent_session(
            &file.path,
            &self.agent_chat_messages,
            &self.deep_agent_last_brief,
            self.deep_learn.as_ref(),
            self.deep_agent_result.as_ref(),
            self.meta_matchup.as_ref(),
        ) {
            logging::record(
                "DECK-HISTORY",
                format!(
                    "AI SESSION WRITE FAILED • deck={} • {err:#}",
                    file.name,
                ),
            );
        }
    }

    fn run_simulation(&mut self) -> Result<()> {
        let Some(parsed) = self.parsed.as_ref() else {
            return Ok(());
        };

        self.status =
            "Running 50,000 local Monte Carlo draw simulations...".to_string();

        match run_draw_simulation(parsed, &self.db, 50_000) {
            Ok(result) => {
                let summary = format!(
                    "{} draws • keepable {:.2}% • avg opening lands {:.2} • 3 lands T3 {:.2}% • early ramp T2 {:.2}%",
                    result.runs,
                    result.keepable_opening_pct,
                    result.average_opening_lands,
                    result.three_lands_by_t3_pct,
                    result.early_ramp_by_t2_pct,
                );

                self.cache_deck_run(
                    "DRAW SIM",
                    summary,
                    vec![
                        format!("4 lands by T4: {:.2}%", result.four_lands_by_t4_pct),
                        format!("5 lands by T5: {:.2}%", result.five_lands_by_t5_pct),
                    ],
                );

                self.simulation = Some(result);
                self.page = Page::Simulation;
                self.status = "50,000 local simulations complete.".to_string();
            }
            Err(err) => self.status = format!("Simulation failed: {err}"),
        }

        Ok(())
    }

    fn run_combo_check(&mut self) -> Result<()> {
        let Some(parsed) = self.parsed.as_ref() else {
            return Ok(());
        };

        self.status =
            "Querying Commander Spellbook's public API...".to_string();

        match find_my_combos(parsed) {
            Ok(report) => {
                if let Some(analysis) = self.analysis.as_mut() {
                    refine_with_combo_report(&mut analysis.bracket, &report);
                }

                let summary = format!(
                    "{} complete combo(s) • {} near-match(es)",
                    report.included,
                    report.almost_included,
                );

                self.cache_deck_run(
                    "COMBO CHECK",
                    summary,
                    report.summary_lines.clone(),
                );

                self.combos = Some(report);
                self.combo_focus_index = 0;
                self.page = Page::Combos;
                self.status = "Commander Spellbook combo check complete.".to_string();
            }
            Err(err) => self.status = format!("Commander Spellbook error: {err}"),
        }

        Ok(())
    }


fn suggestion_names(&self) -> (Vec<String>, Vec<String>) {
    let Some(analysis) = self.analysis.as_ref() else {
        return (Vec::new(), Vec::new());
    };

    (
        analysis
            .cut_candidates
            .iter()
            .map(|item| item.card.clone())
            .collect(),
        analysis
            .add_suggestions
            .iter()
            .map(|item| item.card.clone())
            .collect(),
    )
}

fn focused_suggestion(&self) -> Option<(SuggestionKind, String)> {
    let (cuts, adds) = self.suggestion_names();

    if self.suggestion_focus_column == 0 {
        cuts.get(self.suggestion_focus_index)
            .cloned()
            .map(|name| (SuggestionKind::Cut, name))
    } else {
        adds.get(self.suggestion_focus_index)
            .cloned()
            .map(|name| (SuggestionKind::Add, name))
    }
}

fn prev_suggestion_focus(&mut self) {
    self.suggestion_focus_index =
        self.suggestion_focus_index.saturating_sub(1);
    self.preview_focused_suggestion();
}

fn next_suggestion_focus(&mut self) {
    let (cuts, adds) = self.suggestion_names();
    let count = if self.suggestion_focus_column == 0 {
        cuts.len()
    } else {
        adds.len()
    };

    if count > 0 {
        self.suggestion_focus_index =
            (self.suggestion_focus_index + 1).min(count - 1);
    }

    self.preview_focused_suggestion();
}

fn preview_focused_suggestion(&mut self) {
    if let Some((_, name)) = self.focused_suggestion() {
        self.select_card_by_name(&name);
    }
}

fn select_focused_suggestion(&mut self) {
    let Some((kind, card)) = self.focused_suggestion() else {
        return;
    };

    self.selected_suggestion = Some(SuggestionSelection {
        kind,
        card: card.clone(),
    });
    self.select_card_by_name(&card);

    self.status = match kind {
        SuggestionKind::Cut => format!(
            "Keyboard selected CUT {card}. Press B to move it to sideboard."
        ),
        SuggestionKind::Add => format!(
            "Keyboard selected ADD {card}. Press B to add it to sideboard."
        ),
    };
}

pub fn combo_card_names(&self) -> Vec<String> {
    self.combos
        .as_ref()
        .and_then(|report| report.variants.get(self.combo_focus_index))
        .map(|combo| combo.cards.clone())
        .unwrap_or_default()
}

fn prev_combo_focus(&mut self) {
    self.combo_focus_index =
        self.combo_focus_index.saturating_sub(1);
    self.preview_focused_combo_card();
}

fn next_combo_focus(&mut self) {
    let count = self
        .combos
        .as_ref()
        .map(|report| report.variants.len())
        .unwrap_or(0);

    if count > 0 {
        self.combo_focus_index =
            (self.combo_focus_index + 1).min(count - 1);
    }

    self.preview_focused_combo_card();
}

fn preview_focused_combo_card(&mut self) {
    if let Some(name) =
        self.combo_card_names().first().cloned()
    {
        self.select_card_by_name(&name);
    }
}

fn open_selected_combo(&mut self) {
    let Some(combo) = self
        .combos
        .as_ref()
        .and_then(|report| report.variants.get(self.combo_focus_index))
    else {
        self.status =
            "No Commander Spellbook combo selected.".to_string();
        return;
    };

    let url = combo.spellbook_url.clone();
    self.open_url(&url, "Commander Spellbook combo");
}


fn apply_ai_model_picker_choice(&mut self, index: usize) {
    let mut config = AiConfig::load_or_create();

    if config.set_model_preset(index) {
        let model = config.model.clone();
        self.save_ai_config_status(
            &config,
            format!(
                "AI model set to {model}. Restart Commander Lab to load it.",
            ),
        );
        self.modal = Modal::None;
    }
}

fn save_ai_config_status(
    &mut self,
    config: &AiConfig,
    success: impl Into<String>,
) {
    match config.save() {
        Ok(_) => {
            self.status = success.into();
        }
        Err(err) => {
            self.status =
                format!("Could not save AI settings: {err}");
        }
    }
}

fn apply_selected_theme_preset(&mut self) {
    let Some(named) =
        self.theme_library.get(self.selected_theme_preset).cloned()
    else {
        return;
    };

    theme::apply_named_theme(&named);
    let _ = theme::save_theme("./data/theme.json");
    self.status = format!("Applied theme '{}'.", named.name);
}

fn open_selected_source(&mut self) {
    let Some(source) = self.deck_sources.get(self.selected_source) else {
        self.status = "No source selected.".to_string();
        return;
    };

    let url = source.url.clone();
    let title = source.title.clone();
    self.open_url(&url, &title);
}

fn open_url(&mut self, url: &str, label: &str) {
    if url.trim().is_empty() {
        self.status = format!("{label} has no URL.");
        return;
    }

    match open::that(url) {
        Ok(_) => {
            self.status = format!("Opened {label}.");
        }
        Err(err) => {
            self.status =
                format!("Could not open {label}: {err}");
        }
    }
}

fn parse_requested_run_count(text: &str) -> Option<u32> {
    let patterns = [
        r"(?i)\b(\d[\d,_]*)([km]?)\s*(?:test\s+)?(?:games?|simulations?|sims?|runs?|times?|tests?|matches?|matchups?)\b",
        r"(?i)\b(?:run|simulate|test|playtest)\s+(?:it\s+)?(\d[\d,_]*)([km]?)\b",
    ];

    for pattern in patterns {
        let Ok(regex) = Regex::new(pattern) else {
            continue;
        };

        let Some(captures) = regex.captures(text) else {
            continue;
        };

        let digits = captures
            .get(1)?
            .as_str()
            .chars()
            .filter(|ch| *ch != ',' && *ch != '_')
            .collect::<String>();

        let Ok(base) = digits.parse::<u64>() else {
            continue;
        };

        let multiplier = match captures
            .get(2)
            .map(|value| value.as_str().to_ascii_lowercase())
            .as_deref()
        {
            Some("k") => 1_000u64,
            Some("m") => 1_000_000u64,
            _ => 1u64,
        };

        let requested = base.saturating_mul(multiplier);

        if requested == 0 {
            continue;
        }

        return u32::try_from(requested).ok();
    }

    None
}

fn message_requests_meta_matchups(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();

    [
        "against meta",
        "against the meta",
        "meta decks",
        "meta deck",
        "competitive decks",
        "cedh decks",
        "c edh decks",
        "against cedh",
        "against c edh",
        "pod simulation",
        "pod games",
        "four player",
        "4 player",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

fn requested_agent_intensity(text: &str) -> TestingIntensity {
    let lower = text.to_ascii_lowercase();

    if lower.contains("exhaustive") {
        TestingIntensity::Exhaustive
    } else if lower.contains("quick")
        || lower.contains("fast test")
        || lower.contains("fast testing")
    {
        TestingIntensity::Quick
    } else if lower.contains("standard") {
        TestingIntensity::Standard
    } else {
        // AI Optimization is Thorough unless the user explicitly requests
        // another preset. Exact run counts override the preset simulation
        // count entirely.
        TestingIntensity::Thorough
    }
}

fn parse_requested_candidate_count(text: &str) -> Option<u16> {
    let patterns = [
        r"(?i)\b(\d{1,5})\s*(?:deck\s+)?(?:candidates?|iterations?|versions?|builds?|revisions?|decks?)\b",
        r"(?i)\b(?:try|generate|build|compare|use|make)\s+(\d{1,5})\s+(?:deck\s+)?(?:candidates?|iterations?|versions?|builds?|revisions?|decks?)\b",
    ];

    for pattern in patterns {
        let Ok(regex) = Regex::new(pattern) else {
            continue;
        };

        let Some(captures) = regex.captures(text) else {
            continue;
        };

        let Some(value) = captures.get(1) else {
            continue;
        };

        let Ok(count) = value.as_str().parse::<u16>() else {
            continue;
        };

        if count > 0 {
            return Some(count);
        }
    }

    None
}

fn agent_rounds_for_intensity(
    intensity: TestingIntensity,
) -> u16 {
    match intensity {
        TestingIntensity::Quick => 1,
        TestingIntensity::Standard => 3,
        TestingIntensity::Thorough => 5,
        TestingIntensity::Exhaustive => 8,
    }
}

fn start_deep_agent_with_style(
    &mut self,
    style_brief: String,
) -> Result<()> {
    if self.deep_agent_rx.is_some()
        || self.deep_learn_rx.is_some()
        || self.generation_rx.is_some()
        || self.agent_chat_rx.is_some()
    {
        self.status =
            "Another AI Agent/generation/AI worker is already running."
                .to_string();
        return Ok(());
    }

    let Some(deck) = self
        .deep_agent_result
        .as_ref()
        .map(|result| result.best.deck.clone())
        .or_else(|| self.parsed.clone())
    else {
        self.status =
            "Analyze a deck before starting AI Optimization."
                .to_string();
        return Ok(());
    };

    let attached_references =
        self.attach_references_from_text(&style_brief);

    if !attached_references.is_empty() {
        self.status = format!(
            "Attached {} user reference(s) from the Agent brief.",
            attached_references.len(),
        );
    }

    self.deep_agent_last_brief = style_brief.clone();
    self.deep_agent_style_brief.clear();
    self.deep_agent_prompt_editing = false;
    self.deep_agent_prompt_scroll = 0;

    let lower_brief = style_brief.to_ascii_lowercase();

    let explicit_bracket = (1u8..=5u8).find(|bracket| {
        lower_brief.contains(&format!("bracket {bracket}"))
            || lower_brief.contains(&format!("bracket{bracket}"))
            || lower_brief.contains(&format!("b{bracket} "))
            || lower_brief.ends_with(&format!("b{bracket}"))
    });

    let detected_bracket = self
        .analysis
        .as_ref()
        .map(|analysis| analysis.bracket.bracket)
        .unwrap_or(self.deep_learn_upgrade_bracket)
        .clamp(1, 5);

    let target_bracket =
        explicit_bracket.unwrap_or(detected_bracket);

    self.deep_learn_upgrade_bracket = target_bracket;

    let asks_stax = lower_brief.contains("stax")
        || lower_brief.contains("lock the game")
        || lower_brief.contains("lockdown")
        || lower_brief.contains("prison");

    let rejects_stax = lower_brief.contains("no stax")
        || lower_brief.contains("avoid stax")
        || lower_brief.contains("without stax")
        || lower_brief.contains("not stax");

    let goal = if asks_stax && !rejects_stax {
        OptimizationGoal::StaxLock
    } else {
        OptimizationGoal::FastWin
    };

    self.deep_agent_goal = goal;
    let preferred_archetype = self.selected_archetype_tag.clone();
    let user_references = user_sources(&self.deck_sources);
    let intensity = Self::requested_agent_intensity(&style_brief);
    let exact_final_runs = Self::parse_requested_run_count(&style_brief);

    self.deep_learn_intensity = intensity;

    let rounds = Self::parse_requested_candidate_count(&style_brief)
        .unwrap_or_else(|| Self::agent_rounds_for_intensity(intensity));
    self.deep_agent_rounds = rounds;
    let (tx, rx) = mpsc::channel::<DeepAgentEvent>();

    self.deep_agent_rx = Some(rx);
    self.set_safe_image_mode(true);
    self.deep_agent_result = None;
    self.deep_agent_trace.clear();
    self.deep_agent_progress = 0;
    self.deep_agent_current_task =
        "Preparing optimization session".to_string();
    self.deep_agent_completed_tasks.clear();

    push_trace_log(
        &mut self.deep_agent_trace,
        "AGENT",
        format!(
            "START • deck={} • objective={} • bracket=B{} • testing={} • rounds={} • archetype={} • user_style={}",
            deck.name,
            goal.label(),
            target_bracket,
            exact_final_runs
                .map(|runs| format!("{runs} EXACT RUNS"))
                .unwrap_or_else(|| intensity.label().to_string()),
            rounds,
            preferred_archetype
                .as_deref()
                .unwrap_or("AUTO"),
            style_brief,
        ),
    );

    self.deep_learn_status = format!(
        "Agent researching '{}' then optimizing {} at Bracket {} • {}.",
        style_brief,
        goal.label(),
        target_bracket,
        exact_final_runs
            .map(|runs| format!("{runs} requested simulations"))
            .unwrap_or_else(|| {
                format!("{} testing", intensity.label())
            }),
    );

    thread::spawn(move || {
        let result = (|| -> Result<DeepAgentResult> {
            let db = CardDb::open("./data/cards.sqlite")?;

            run_optimization_agent(
                &deck,
                &db,
                target_bracket,
                goal,
                preferred_archetype,
                Some(style_brief),
                user_references,
                intensity,
                exact_final_runs,
                rounds,
                |status| {
                    let _ = tx.send(
                        DeepAgentEvent::Progress(status)
                    );
                },
            )
        })();

        let _ = tx.send(DeepAgentEvent::Finished(
            result.map_err(|err| format!("{err:#}")),
        ));
    });

    Ok(())
}

fn deck_comparison(
    original: &ParsedDeck,
    optimized: &ParsedDeck,
) -> DeckComparison {
    fn collect(
        deck: &ParsedDeck,
    ) -> HashMap<(String, String), (String, u32)> {
        let mut map =
            HashMap::<(String, String), (String, u32)>::new();

        for (zone, entries) in [
            ("MAIN", &deck.main),
            ("SIDE", &deck.sideboard),
        ] {
            for entry in entries {
                let key = (
                    zone.to_string(),
                    entry.name.to_ascii_lowercase(),
                );

                let current = map
                    .entry(key)
                    .or_insert((entry.name.clone(), 0));

                current.1 =
                    current.1.saturating_add(entry.quantity);
            }
        }

        map
    }

    let before = collect(original);
    let after = collect(optimized);
    let mut added = Vec::<DeckComparisonEntry>::new();
    let mut removed = Vec::<DeckComparisonEntry>::new();
    let mut unchanged_main = 0usize;

    for ((zone, key), (name, after_qty)) in &after {
        let before_qty = before
            .get(&(zone.clone(), key.clone()))
            .map(|(_, qty)| *qty)
            .unwrap_or(0);

        if *after_qty > before_qty {
            added.push(DeckComparisonEntry {
                quantity: after_qty - before_qty,
                name: name.clone(),
                zone: zone.clone(),
            });
        }

        if zone == "MAIN" && before_qty > 0 {
            unchanged_main += 1;
        }
    }

    for ((zone, key), (name, before_qty)) in &before {
        let after_qty = after
            .get(&(zone.clone(), key.clone()))
            .map(|(_, qty)| *qty)
            .unwrap_or(0);

        if *before_qty > after_qty {
            removed.push(DeckComparisonEntry {
                quantity: before_qty - after_qty,
                name: name.clone(),
                zone: zone.clone(),
            });
        }
    }

    added.sort_by(|a, b| {
        a.zone.cmp(&b.zone).then_with(|| a.name.cmp(&b.name))
    });
    removed.sort_by(|a, b| {
        a.zone.cmp(&b.zone).then_with(|| a.name.cmp(&b.name))
    });

    DeckComparison {
        original_name: original.name.clone(),
        optimized_name: optimized.name.clone(),
        added,
        removed,
        unchanged_main,
        original_main_count:
            original.main.iter().map(|entry| entry.quantity).sum(),
        optimized_main_count:
            optimized.main.iter().map(|entry| entry.quantity).sum(),
    }
}

fn sync_agent_workspace(
    &mut self,
    agent: &DeepAgentResult,
) -> Result<()> {
    let original = self.parsed.clone();

    let Some(original) = original else {
        return Ok(());
    };

    let comparison =
        Self::deck_comparison(&original, &agent.best.deck);

    let mut research = agent.research.clone();

    // Accepted Agent changes outrank remaining learned suggestions.
    let mut actual_adds = comparison
        .added
        .iter()
        .filter(|entry| entry.zone == "MAIN")
        .map(|entry| DeepLearnCard {
            name: entry.name.clone(),
            score: 1000,
            confidence: research.confidence.max(80),
            reason: format!(
                "AI Optimization added {} copy/copies to the final accepted deck",
                entry.quantity,
            ),
        })
        .collect::<Vec<_>>();

    let mut actual_cuts = comparison
        .removed
        .iter()
        .filter(|entry| entry.zone == "MAIN")
        .map(|entry| DeepLearnCard {
            name: entry.name.clone(),
            score: 1000,
            confidence: research.confidence.max(80),
            reason: format!(
                "AI Optimization removed {} copy/copies from the final accepted deck",
                entry.quantity,
            ),
        })
        .collect::<Vec<_>>();

    for item in research.adds {
        if !actual_adds.iter().any(|existing| {
            existing.name.eq_ignore_ascii_case(&item.name)
        }) {
            actual_adds.push(item);
        }
    }

    for item in research.cuts {
        if !actual_cuts.iter().any(|existing| {
            existing.name.eq_ignore_ascii_case(&item.name)
        }) {
            actual_cuts.push(item);
        }
    }

    research.adds = actual_adds;
    research.cuts = actual_cuts;
    research.testing = Some(agent.final_testing.clone());

    if !agent.ai_profile.commander_summary.trim().is_empty() {
        research.findings.insert(
            0,
            format!(
                "AI commander model: {}",
                agent.ai_profile.commander_summary.trim(),
            ),
        );
    }

    if !agent.ai_profile.gameplan.is_empty() {
        research.findings.insert(
            1.min(research.findings.len()),
            format!(
                "AI game plan: {}",
                agent.ai_profile.gameplan.join(" → "),
            ),
        );
    }

    if !agent.ai_profile.recommended_archetype.trim().is_empty()
        && !research.normal_themes.iter().any(|theme| {
            theme.eq_ignore_ascii_case(
                &agent.ai_profile.recommended_archetype,
            )
        })
    {
        research.normal_themes.insert(
            0,
            agent.ai_profile.recommended_archetype.clone(),
        );
    }

    let original_main = original
        .main
        .iter()
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();

    for name in &agent.ai_profile.priority_cards {
        if !research.adds.iter().any(|item| {
            item.name.eq_ignore_ascii_case(name)
        }) {
            research.adds.push(DeepLearnCard {
                name: name.clone(),
                score: 900,
                confidence: research.confidence.max(75),
                reason:
                    "AI Optimization priority card".to_string(),
            });
        }
    }

    for name in &agent.ai_profile.avoid_or_question_cards {
        if original_main.contains(&name.to_ascii_lowercase())
            && !research.cuts.iter().any(|item| {
                item.name.eq_ignore_ascii_case(name)
            })
        {
            research.cuts.push(DeepLearnCard {
                name: name.clone(),
                score: 900,
                confidence: research.confidence.max(75),
                reason:
                    "AI Optimization flagged this card for review"
                        .to_string(),
            });
        }
    }

    for finding in agent
        .ai_profile
        .community_findings
        .iter()
        .chain(agent.ai_profile.evidence_notes.iter())
        .take(10)
    {
        let line = format!("AI evidence: {finding}");
        if !research.findings.contains(&line) {
            research.findings.push(line);
        }
    }

    for package in &agent.ai_profile.win_packages {
        let line = format!(
            "{} — {}",
            package.cards.join(" + "),
            package.name,
        );

        if !research.reference_combos.iter().any(|existing| {
            existing.eq_ignore_ascii_case(&line)
        }) {
            research.reference_combos.push(line);
        }
    }

    // Merge Agent evidence without dropping user-provided sources.
    let mut merged_sources = self.deck_sources.clone();

    for source in research
        .sources
        .iter()
        .chain(agent.ai_profile.sources.iter())
    {
        if !merged_sources.iter().any(|existing| {
            existing.url.eq_ignore_ascii_case(&source.url)
                && existing.title.eq_ignore_ascii_case(&source.title)
        }) {
            merged_sources.push(source.clone());
        }
    }

    self.deck_sources = merged_sources;
    self.selected_source =
        self.selected_source.min(self.deck_sources.len().saturating_sub(1));
    self.source_scroll = 0;

    // Persist the merged evidence corpus with the deck.
    self.persist_current_sources()?;

    self.deep_learn = Some(research);
    self.deep_learn_testing =
        Some(agent.final_testing.clone());
    self.deep_agent_comparison = Some(comparison);
    self.compare_scroll = 0;

    Ok(())
}

fn poll_deep_agent_events(&mut self) -> Result<()> {
    let mut finished = None;

    if let Some(rx) = &self.deep_agent_rx {
        loop {
            match rx.try_recv() {
                Ok(DeepAgentEvent::Progress(update)) => {
                    let task_changed =
                        self.deep_agent_current_task != update.task;

                    self.deep_agent_progress =
                        update.percent.min(100);

                    if task_changed {
                        self.deep_agent_current_task =
                            update.task.clone();

                        push_trace_log(
                            &mut self.deep_agent_trace,
                            "AGENT",
                            format!(
                                "WORKING • {}",
                                update.task,
                            ),
                        );
                    }

                    if let Some(completed) =
                        update.completed.as_ref()
                    {
                        let line =
                            format!("DONE • {completed}");

                        if self
                            .deep_agent_completed_tasks
                            .last()
                            .map(|last| last != &line)
                            .unwrap_or(true)
                        {
                            self.deep_agent_completed_tasks
                                .push(line.clone());

                            if self.deep_agent_completed_tasks.len()
                                > 12
                            {
                                let remove =
                                    self.deep_agent_completed_tasks.len()
                                        - 12;
                                self.deep_agent_completed_tasks
                                    .drain(0..remove);
                            }

                            push_trace_log(
                                &mut self.deep_agent_trace,
                                "AGENT",
                                &line,
                            );
                        }
                    }

                    let clean_status = if update.detail.trim().is_empty() {
                        format!(
                            "{} • {}%",
                            update.task,
                            update.percent,
                        )
                    } else {
                        format!(
                            "{} • {}% • {}",
                            update.task,
                            update.percent,
                            update.detail,
                        )
                    };

                    self.deep_learn_status =
                        clean_status.clone();
                    self.deep_agent_prompt_scroll = usize::MAX;
                    self.status = format!(
                        "AI Optimization • {clean_status}"
                    );
                }

                Ok(DeepAgentEvent::Finished(result)) => {
                    finished = Some(result);
                    break;
                }

                Err(TryRecvError::Empty) => break,

                Err(TryRecvError::Disconnected) => {
                    push_trace_log(&mut self.deep_agent_trace, "AGENT", 
                        "ERROR • worker disconnected unexpectedly",
                    );
                    self.deep_agent_rx = None;
                    self.deep_learn_status =
                        "AI Optimization disconnected unexpectedly."
                            .to_string();
                    break;
                }
            }
        }
    }

    if let Some(result) = finished {
        self.deep_agent_rx = None;

        match result {
            Ok(agent) => {
                push_trace_log(&mut self.deep_agent_trace, "AGENT", format!(
                    "SUCCESS • objective={} • B{} • rounds={} • final_games={} • final_earliest={} • saved_candidate={}",
                    agent.goal.label(),
                    agent.target_bracket,
                    agent.rounds.len(),
                    agent.final_testing.goldfish.runs,
                    agent.final_testing.goldfish
                        .earliest_combo_turn
                        .map(|turn| format!("T{turn}"))
                        .unwrap_or_else(|| "none".to_string()),
                    agent.best.deck.name,
                ));
                self.sync_agent_workspace(&agent)?;
                for line in &agent.trace {
                    push_trace_log(
                        &mut self.deep_agent_trace,
                        "AGENT-DECISION",
                        line,
                    );
                }
                self.deep_agent_progress = 100;
                self.deep_agent_current_task =
                    "Optimization complete".to_string();
                self.deep_agent_prompt_scroll = usize::MAX;

                self.deep_learn_status = format!(
                    "Agent complete • {} • B{} • {} round(s).",
                    agent.goal.label(),
                    agent.target_bracket,
                    agent.rounds.len(),
                );
                self.status = self.deep_learn_status.clone();

                self.cache_deck_run(
                    "AI OPTIMIZATION",
                    format!(
                        "{} • target B{} • {} rounds • {} final games • earliest {}",
                        agent.goal.label(),
                        agent.target_bracket,
                        agent.rounds.len(),
                        agent.final_testing.goldfish.runs,
                        agent.final_testing.goldfish.earliest_combo_turn
                            .map(|turn| format!("T{turn}"))
                            .unwrap_or_else(|| "none".to_string()),
                    ),
                    self.deep_agent_trace.clone(),
                );

                let chat_summary = format!(
                    "AI Optimization complete: {} • target B{} • {} optimization round(s) • final earliest {} • T5 {:.2}%.",
                    agent.goal.label(),
                    agent.target_bracket,
                    agent.rounds.len(),
                    agent.final_testing.goldfish
                        .earliest_combo_turn
                        .map(|turn| format!("T{turn}"))
                        .unwrap_or_else(|| "none by T8".to_string()),
                    agent.final_testing.goldfish.combo_by_t5_pct,
                );

                self.deep_agent_result = Some(agent);

                if self.deep_learn_page == DeepLearnPage::Chat {
                    self.agent_chat_messages.push(
                        AgentChatMessage {
                            role: "system".to_string(),
                            content: chat_summary,
                        },
                    );
                    self.agent_chat_status =
                        "Optimization complete. Continue chatting to refine it, then say \"run optimization\" when ready."
                            .to_string();
                    self.agent_chat_scroll = 0;
                } else {
                    self.deep_learn_page = DeepLearnPage::Agent;
                }

                self.persist_ai_session();

                if self.agent_chat_simulate_after_optimize {
                    self.agent_chat_simulate_after_optimize = false;
                    self.agent_chat_status =
                        "Deck changes complete. Running the requested playtest."
                            .to_string();
                    self.run_pending_agent_simulation()?;
                } else if let Some(instruction) =
                    self.agent_chat_follow_up.take()
                {
                    self.start_agent_chat_with_message(
                        instruction,
                    )?;
                }
            }

            Err(err) => {
                push_trace_log(&mut self.deep_agent_trace, "AGENT", format!(
                    "FAILED • {err}",
                ));
                self.deep_agent_current_task =
                    "Optimization failed".to_string();
                self.deep_learn_status =
                    format!("AI Optimization failed: {err}");
                self.status = self.deep_learn_status.clone();

                if self.deep_learn_page == DeepLearnPage::Chat {
                    self.agent_chat_messages.push(
                        AgentChatMessage {
                            role: "system".to_string(),
                            content: format!(
                                "AI Optimization failed: {err}"
                            ),
                        },
                    );
                    self.agent_chat_status =
                        "Optimization failed. Adjust the direction or inspect TRACE, then rerun."
                            .to_string();
                }

                self.persist_ai_session();
            }
        }
    }

    Ok(())
}

fn persist_current_sources(&mut self) -> Result<()> {
    let Some(deck_file) = self.decks.get(self.selected_deck) else {
        return Ok(());
    };

    save_sources_for_deck(
        &deck_file.path,
        &self.deck_sources,
    )?;

    Ok(())
}

fn add_user_reference_url(
    &mut self,
    url: &str,
) -> Result<bool> {
    let trimmed = url.trim();

    if trimmed.is_empty() {
        return Ok(false);
    }

    if self.deck_sources.iter().any(|source| {
        source.url.eq_ignore_ascii_case(trimmed)
    }) {
        self.status =
            format!("Reference already attached: {trimmed}");
        return Ok(false);
    }

    self.status =
        format!("Importing user reference: {trimmed}");

    let source = import_user_reference(trimmed)?;

    self.deck_sources.push(source.clone());
    self.selected_source =
        self.deck_sources.len().saturating_sub(1);

    self.persist_current_sources()?;

    logging::record(
        "USER-REF",
        format!(
            "ATTACHED • deck={} • source={} • title={} • url={} • embedded_deck={}",
            self.parsed
                .as_ref()
                .map(|deck| deck.name.as_str())
                .unwrap_or("unknown"),
            source.source,
            source.title,
            source.url,
            source.reference_deck.is_some(),
        ),
    );

    self.status = format!(
        "Added user reference: {}",
        source.title,
    );

    Ok(true)
}

fn attach_references_from_text(
    &mut self,
    text: &str,
) -> Vec<String> {
    let urls = extract_urls(text);
    let mut attached = Vec::new();

    for url in urls {
        match self.add_user_reference_url(&url) {
            Ok(true) => attached.push(url),
            Ok(false) => {}
            Err(err) => {
                logging::record(
                    "USER-REF",
                    format!(
                        "AUTO-ATTACH FAILED • url={} • error={:#}",
                        url,
                        err,
                    ),
                );

                self.status = format!(
                    "Could not import reference {}: {:#}",
                    url,
                    err,
                );
            }
        }
    }

    attached
}

fn remove_selected_user_reference(&mut self) -> Result<()> {
    let Some(source) =
        self.deck_sources.get(self.selected_source).cloned()
    else {
        return Ok(());
    };

    if !source.user_provided {
        self.status =
            "Only references added by the user can be removed here."
                .to_string();
        return Ok(());
    }

    self.deck_sources.remove(self.selected_source);

    self.selected_source = self
        .selected_source
        .min(self.deck_sources.len().saturating_sub(1));

    self.persist_current_sources()?;

    logging::record(
        "USER-REF",
        format!(
            "REMOVED • title={} • url={}",
            source.title,
            source.url,
        ),
    );

    self.status =
        format!("Removed user reference: {}", source.title);

    Ok(())
}

fn invalidate_current_research_snapshot(&mut self) {
    let deck = self
        .deep_agent_result
        .as_ref()
        .map(|result| result.best.deck.clone())
        .or_else(|| self.parsed.clone());

    let Some(deck) = deck else {
        return;
    };

    let commanders = deck
        .commanders
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();

    if commanders.is_empty() {
        return;
    }

    let bracket = self
        .deep_agent_result
        .as_ref()
        .map(|result| result.target_bracket)
        .or_else(|| {
            self.analysis
                .as_ref()
                .map(|analysis| analysis.bracket.bracket)
        })
        .unwrap_or(self.deep_learn_upgrade_bracket)
        .clamp(1, 5);

    if let Err(err) =
        invalidate_reference_snapshot(&commanders, bracket)
    {
        logging::record(
            "REF",
            format!(
                "FORCED REFRESH FAILED • {}",
                err,
            ),
        );
    }
}

fn message_requests_decklist(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();

    [
        "decklist",
        "deck list",
        "show deck",
        "print deck",
        "export deck",
        "output deck",
        "list the deck",
        "full list",
        "show me the list",
        "show the list",
        "copy the deck",
        "give me the list",
        "write out the deck",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
}

fn message_is_decklist_only(text: &str) -> bool {
    if !Self::message_requests_decklist(text) {
        return false;
    }

    let lower = text.to_ascii_lowercase();
    let stripped = lower
        .replace("please", "")
        .replace("can you", "")
        .replace("could you", "")
        .replace("show me", "")
        .replace("give me", "")
        .replace("the", "")
        .replace("my", "")
        .replace("current", "")
        .replace("full", "")
        .replace("decklist", "")
        .replace("deck list", "")
        .replace("deck", "")
        .replace("list", "")
        .replace("export", "")
        .replace("output", "")
        .replace("print", "")
        .replace("copy", "")
        .replace("write out", "")
        .replace('.', "")
        .replace('?', "")
        .replace('!', "")
        .trim()
        .to_string();

    stripped.chars().filter(|ch| !ch.is_whitespace()).count() <= 4
}

fn agent_chat_deck(&self) -> Option<ParsedDeck> {
    self.deep_agent_result
        .as_ref()
        .map(|result| result.best.deck.clone())
        .or_else(|| self.parsed.clone())
}

fn respond_agent_decklist(&mut self) -> Result<()> {
    let Some(deck) = self.agent_chat_deck() else {
        self.agent_chat_status =
            "Analyze a deck before requesting a decklist.".to_string();
        return Ok(());
    };

    let text = deck_to_text(&deck);

    self.agent_chat_messages.push(AgentChatMessage {
        role: "assistant".to_string(),
        content: format!(
            "Current decklist — {}:\n\n{text}",
            deck.name,
        ),
    });

    if self.agent_chat_messages.len() > 80 {
        let remove = self.agent_chat_messages.len() - 80;
        self.agent_chat_messages.drain(0..remove);
    }

    self.agent_chat_scroll = 0;
    self.agent_chat_status =
        "Decklist ready. Continue chatting or say \"run optimization\" to apply changes."
            .to_string();
    self.persist_ai_session();

    Ok(())
}

fn plan_agent_instruction(message: &str) -> AgentInstructionPlan {
    let lower = message.to_ascii_lowercase();

    let has_run_verb = [
        "run ",
        "start ",
        "execute ",
        "launch ",
        "begin ",
    ]
    .iter()
    .any(|verb| lower.contains(verb));

    let research = [
        "run research",
        "start research",
        "do research",
        "run deep learn",
        "start deep learn",
        "refresh research",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase));

    let simulate = [
        "run simulation",
        "run sim",
        "run playtest",
        "start simulation",
        "start playtest",
        "run games",
        "run the games",
        "test the deck",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
        || (Self::parse_requested_run_count(message).is_some()
            && has_run_verb);

    let optimize = [
        "run optimization",
        "run optimise",
        "start optimization",
        "start optimise",
        "optimize now",
        "optimise now",
        "optimize the deck",
        "optimise the deck",
        "run the agent",
        "run agent",
        "start agent",
        "start the agent",
        "apply changes",
        "apply that",
        "apply these",
        "apply those",
        "make those changes",
        "execute optimization",
        "run minmax",
        "run min-max",
        "do the optimization",
        "do optimization",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase))
        || (Self::parse_requested_candidate_count(message).is_some()
            && has_run_verb);

    if !research && !simulate && !optimize {
        return AgentInstructionPlan::converse(
            message.to_string(),
        );
    }

    let simulate_position = [
        "run simulation",
        "run sim",
        "run playtest",
        "run games",
    ]
    .iter()
    .filter_map(|phrase| lower.find(phrase))
    .min();

    let optimize_position = [
        "run optimization",
        "run optimise",
        "run agent",
        "run minmax",
        "apply changes",
    ]
    .iter()
    .filter_map(|phrase| lower.find(phrase))
    .min();

    let simulate_before_optimize =
        simulate
            && optimize
            && simulate_position
                .zip(optimize_position)
                .map(|(sim, opt)| sim < opt)
                .unwrap_or(false)
            && lower.contains("then");

    AgentInstructionPlan {
        research,
        optimize,
        simulate,
        simulate_before_optimize,
        meta_matchups:
            Self::message_requests_meta_matchups(message),
        exact_runs: Self::parse_requested_run_count(message),
        intensity: Self::requested_agent_intensity(message),
        follow_up: false,
        original: message.to_string(),
    }
}

fn open_agent_chat_tab(&mut self) {
    self.screen = Screen::DeepLearn;
    self.deep_learn_page = DeepLearnPage::Chat;
    self.ensure_agent_chat_greeting();
}

fn ensure_agent_chat_greeting(&mut self) {
    if !self.agent_chat_messages.is_empty() {
        return;
    }

    let deck_hint = self
        .parsed
        .as_ref()
        .map(|deck| {
            let commanders = deck
                .commanders
                .iter()
                .map(|entry| entry.name.clone())
                .collect::<Vec<_>>()
                .join(" + ");

            format!(
                "I loaded {name} ({commanders}) — {count} maindeck cards.",
                name = deck.name,
                commanders = commanders,
                count = deck.main.len(),
            )
        })
        .unwrap_or_else(|| {
            "Load a deck first so I can read the list.".to_string()
        });

    self.agent_chat_messages.push(AgentChatMessage {
        role: "assistant".to_string(),
        content: format!(
            "{deck_hint}\n\n\
            I'm here to help shape the game plan before we change anything.\n\n\
            A few things that will help me help you:\n\
            • What should this deck *do* — fast combo, grindy value, stax, battlecruiser?\n\
            • What Bracket or power level are you aiming for?\n\
            • What must stay (pet cards, themes, combos)?\n\
            • What's felt wrong in games — speed, consistency, interaction, mana?\n\n\
            Talk through it with me like we're pair-building. I'll ask follow-ups until we're aligned. \
            When you're ready, say \"run optimization\" to apply changes, \"run simulation\" to playtest, \
            or \"show decklist\" anytime.",
        ),
    });

    self.agent_chat_scroll = 0;
    self.persist_ai_session();
}

fn clear_agent_command_chain(&mut self) {
    self.agent_chat_optimize_after_research = false;
    self.agent_chat_simulate_after_research = false;
    self.agent_chat_simulate_after_optimize = false;
    self.agent_chat_optimize_after_simulation = false;
    self.agent_chat_pending_runs = None;
    self.agent_chat_pending_intensity =
        TestingIntensity::Thorough;
    self.agent_chat_pending_meta = false;
    self.agent_chat_follow_up = None;
}

fn queue_agent_simulation(
    &mut self,
    plan: &AgentInstructionPlan,
) {
    self.agent_chat_pending_runs = plan.exact_runs;
    self.agent_chat_pending_intensity = plan.intensity;
    self.agent_chat_pending_meta = plan.meta_matchups;
}

fn run_pending_agent_simulation(&mut self) -> Result<()> {
    let runs = self.agent_chat_pending_runs.take();
    let intensity = self.agent_chat_pending_intensity;
    let meta = self.agent_chat_pending_meta;
    self.agent_chat_pending_meta = false;

    self.start_agent_requested_simulation(
        intensity,
        runs,
        meta,
    )
}

fn commit_agent_chat_input(&mut self) -> Option<String> {
    let message = self.agent_chat_input.trim().to_string();

    if message.is_empty() {
        return None;
    }

    let _ = self.attach_references_from_text(&message);
    self.agent_chat_input.clear();

    self.agent_chat_messages.push(AgentChatMessage {
        role: "user".to_string(),
        content: message.clone(),
    });

    if self.agent_chat_messages.len() > 80 {
        let remove = self.agent_chat_messages.len() - 80;
        self.agent_chat_messages.drain(0..remove);
    }

    self.agent_chat_scroll = 0;
    self.persist_ai_session();
    Some(message)
}

fn handle_agent_chat_submit(&mut self) -> Result<()> {
    if self.agent_chat_rx.is_some()
        || self.deep_agent_rx.is_some()
        || self.deep_learn_rx.is_some()
        || self.generation_rx.is_some()
    {
        self.agent_chat_status =
            "Wait for the current AI task to finish.".to_string();
        return Ok(());
    }

    let Some(message) = self.commit_agent_chat_input() else {
        return Ok(());
    };

    let plan = Self::plan_agent_instruction(&message);

    if Self::message_is_decklist_only(&message) {
        return self.respond_agent_decklist();
    }

    if !plan.has_work() {
        return self.start_agent_chat_with_message(message);
    }

    self.clear_agent_command_chain();

    if plan.follow_up {
        self.agent_chat_follow_up =
            Some(plan.original.clone());
    }

    self.queue_agent_simulation(&plan);

    self.deep_learn_page = DeepLearnPage::Chat;

    if plan.research {
        self.invalidate_current_research_snapshot();
        if plan.simulate_before_optimize {
            self.agent_chat_simulate_after_research =
                plan.simulate;
            self.agent_chat_optimize_after_simulation =
                plan.optimize;
        } else {
            self.agent_chat_optimize_after_research =
                plan.optimize;
            self.agent_chat_simulate_after_research =
                plan.simulate && !plan.optimize;
            self.agent_chat_simulate_after_optimize =
                plan.simulate && plan.optimize;
        }

        self.agent_chat_status =
            "Agent is researching the instruction first."
                .to_string();
        self.start_deep_learn()?;
        return Ok(());
    }

    if plan.optimize && plan.simulate_before_optimize {
        self.agent_chat_optimize_after_simulation = true;
        self.agent_chat_status =
            "Agent is running the requested playtest before applying deck changes."
                .to_string();
        self.run_pending_agent_simulation()?;
        return Ok(());
    }

    if plan.optimize {
        self.agent_chat_simulate_after_optimize =
            plan.simulate;
        self.agent_chat_status =
            "Agent is applying the instruction to the deck."
                .to_string();
        self.run_agent_from_chat()?;
        return Ok(());
    }

    if plan.simulate {
        self.agent_chat_status =
            "Agent is running the requested playtest."
                .to_string();
        self.run_pending_agent_simulation()?;
        return Ok(());
    }

    self.start_agent_chat_with_message(message)
}

fn run_agent_from_chat(&mut self) -> Result<()> {
    if self.deep_agent_rx.is_some()
        || self.deep_learn_rx.is_some()
        || self.generation_rx.is_some()
        || self.agent_chat_rx.is_some()
    {
        self.agent_chat_status =
            "Wait for the current worker to finish before rerunning the Agent."
                .to_string();
        return Ok(());
    }

    let pending = self.agent_chat_input.trim().to_string();

    if !pending.is_empty() {
        let _ = self.attach_references_from_text(&pending);

        self.agent_chat_messages.push(AgentChatMessage {
            role: "user".to_string(),
            content: pending,
        });

        self.agent_chat_input.clear();
    }

    let user_direction = self
        .agent_chat_messages
        .iter()
        .filter(|message| message.role == "user")
        .rev()
        .take(20)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|message| message.content)
        .collect::<Vec<_>>();

    let mut brief = self.deep_agent_last_brief.trim().to_string();

    if !user_direction.is_empty() {
        if !brief.is_empty() {
            brief.push_str(
                "\n\nADDITIONAL DIRECTION FROM AGENT CHAT:\n",
            );
        }

        brief.push_str(&user_direction.join("\n"));
    }

    if brief.trim().is_empty() {
        self.agent_chat_status =
            "Tell me how you want the deck built in chat first, then say \"run optimization\" when you're ready."
                .to_string();
        return Ok(());
    }

    self.deep_agent_last_brief = brief.clone();
    self.deep_agent_style_brief.clear();

    self.agent_chat_messages.push(AgentChatMessage {
        role: "system".to_string(),
        content: format!(
            "Starting AI Optimization with the current chat direction at {} thoroughness.",
            self.deep_learn_intensity.label(),
        ),
    });

    self.agent_chat_scroll = 0;
    self.start_deep_agent_with_style(brief)?;
    self.deep_learn_page = DeepLearnPage::Chat;
    self.agent_chat_status =
        "AI Optimization is running with the current conversation as direction."
            .to_string();

    Ok(())
}

fn start_agent_chat_with_message(
    &mut self,
    message: String,
) -> Result<()> {
    if self.agent_chat_rx.is_some() {
        self.agent_chat_status =
            "The Agent is already answering.".to_string();
        return Ok(());
    }

    if self.deep_agent_rx.is_some()
        || self.deep_learn_rx.is_some()
        || self.generation_rx.is_some()
    {
        self.agent_chat_status =
            "Wait for the current AI task to finish.".to_string();
        return Ok(());
    }

    let Some(deck) = self
        .deep_agent_result
        .as_ref()
        .map(|result| result.best.deck.clone())
        .or_else(|| self.parsed.clone())
    else {
        self.agent_chat_status =
            "Analyze a deck before opening Agent chat.".to_string();
        return Ok(());
    };

    let exclude_last_user_echo = self
        .agent_chat_messages
        .last()
        .map(|last| {
            last.role == "user"
                && last.content.trim() == message.trim()
        })
        .unwrap_or(false);

    let history_len = if exclude_last_user_echo {
        self.agent_chat_messages.len().saturating_sub(1)
    } else {
        self.agent_chat_messages.len()
    };

    let history = self
        .agent_chat_messages
        .iter()
        .take(history_len)
        .cloned()
        .collect::<Vec<_>>();

    let research = self.deep_learn.clone();
    let user_references = user_sources(&self.deck_sources);
    let profile = self
        .deep_agent_result
        .as_ref()
        .map(|result| result.ai_profile.clone());

    let execution_context = {
        let mut lines = Vec::<String>::new();

        if let Some(agent) = self.deep_agent_result.as_ref() {
            lines.push(format!(
                "Last optimization: objective={} target=B{} rounds={} final_games={}",
                agent.goal.label(),
                agent.target_bracket,
                agent.rounds.len(),
                agent.final_testing.goldfish.runs,
            ));

            for (rank, iteration) in
                agent.top_iterations.iter().take(5).enumerate()
            {
                lines.push(format!(
                    "Candidate #{}: round={} score={:.3} bracket=B{} earliest={} T4={:.2}% T5={:.2}%",
                    rank + 1,
                    iteration.round,
                    iteration.objective_score,
                    iteration.detected_bracket,
                    iteration
                        .testing
                        .goldfish
                        .earliest_combo_turn
                        .map(|turn| format!("T{turn}"))
                        .unwrap_or_else(|| "none".to_string()),
                    iteration.testing.goldfish.combo_by_t4_pct,
                    iteration.testing.goldfish.combo_by_t5_pct,
                ));
            }
        }

        if let Some(test) = self.deep_learn_testing.as_ref() {
            lines.push(format!(
                "Last playtest: games={} keepable={:.2}% manaT3={:.2}% manaT4={:.2}% comboT4={:.2}% comboT5={:.2}% comboT6={:.2}%",
                test.goldfish.runs,
                test.goldfish.keepable_pct,
                test.goldfish.three_mana_t3_pct,
                test.goldfish.four_mana_t4_pct,
                test.goldfish.combo_by_t4_pct,
                test.goldfish.combo_by_t5_pct,
                test.goldfish.combo_by_t6_pct,
            ));
        }

        if let Some(meta) = self.meta_matchup.as_ref() {
            lines.push(format!(
                "Last meta test: games={} opponents={} outright={:.2}% equity={:.2}% shared_first={:.2}% loss={:.2}% avg_win_turn={}",
                meta.runs,
                meta.opponents_used,
                meta.outright_win_pct,
                meta.equity_win_pct,
                meta.shared_first_pct,
                meta.loss_pct,
                meta.average_winning_turn
                    .map(|turn| format!("{turn:.2}"))
                    .unwrap_or_else(|| "n/a".to_string()),
            ));
        }

        if lines.is_empty() {
            "No previous Agent execution results.".to_string()
        } else {
            lines.join("\n")
        }
    };

    let style = if self.deep_agent_last_brief.trim().is_empty() {
        None
    } else {
        Some(self.deep_agent_last_brief.clone())
    };

    let (tx, rx) = mpsc::channel::<AgentChatEvent>();
    self.agent_chat_rx = Some(rx);
    self.set_safe_image_mode(true);
    self.agent_chat_status =
        "Agent is thinking..."
            .to_string();

    thread::spawn(move || {
        let result = (|| -> Result<String> {
            let db = CardDb::open("./data/cards.sqlite")?;

            chat_with_deck_agent(
                &deck,
                &db,
                research.as_ref(),
                profile.as_ref(),
                style.as_deref(),
                &user_references,
                &history,
                &execution_context,
                &message,
            )
        })();

        let _ = tx.send(AgentChatEvent::Finished(
            result.map_err(|err| format!("{err:#}")),
        ));
    });

    Ok(())
}


fn poll_agent_chat_events(&mut self) -> Result<()> {
    let mut finished = None;

    if let Some(rx) = &self.agent_chat_rx {
        match rx.try_recv() {
            Ok(AgentChatEvent::Finished(result)) => {
                finished = Some(result);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                finished = Some(Err(
                    "Local Agent chat worker disconnected unexpectedly."
                        .to_string(),
                ));
            }
        }
    }

    if let Some(result) = finished {
        self.agent_chat_rx = None;

        match result {
            Ok(answer) => {
                self.agent_chat_messages.push(AgentChatMessage {
                    role: "assistant".to_string(),
                    content: answer,
                });

                if self.agent_chat_messages.len() > 80 {
                    let remove =
                        self.agent_chat_messages.len() - 80;
                    self.agent_chat_messages.drain(0..remove);
                }

                self.agent_chat_status =
                    "Agent response complete.".to_string();
                self.persist_ai_session();
            }
            Err(err) => {
                self.agent_chat_messages.push(AgentChatMessage {
                    role: "system".to_string(),
                    content: format!(
                        "Local AI chat failed: {err}"
                    ),
                });
                self.agent_chat_status =
                    "Local Agent chat failed.".to_string();
                self.persist_ai_session();
            }
        }
    }

    Ok(())
}

fn clear_agent_chat(&mut self) {
    if self.agent_chat_rx.is_some() {
        self.agent_chat_status =
            "Wait for the current Agent response before clearing."
                .to_string();
        return;
    }

    self.agent_chat_messages.clear();
    self.agent_chat_input.clear();
    self.clear_agent_command_chain();
    self.agent_chat_status =
        "Conversation cleared. Tell me how you want the deck built."
            .to_string();
    self.ensure_agent_chat_greeting();
    self.persist_ai_session();
}

fn handle_agent_chat_key(&mut self, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            self.screen = Screen::Dashboard;
        }
        KeyCode::Tab => {
            let current = DeepLearnPage::ALL
                .iter()
                .position(|page| *page == self.deep_learn_page)
                .unwrap_or(0);

            self.deep_learn_page = DeepLearnPage::ALL[
                (current + 1) % DeepLearnPage::ALL.len()
            ];

            if self.deep_learn_page == DeepLearnPage::Chat {
                self.ensure_agent_chat_greeting();
            }
        }
        KeyCode::BackTab => {
            let current = DeepLearnPage::ALL
                .iter()
                .position(|page| *page == self.deep_learn_page)
                .unwrap_or(0);

            self.deep_learn_page = if current == 0 {
                DeepLearnPage::ALL[DeepLearnPage::ALL.len() - 1]
            } else {
                DeepLearnPage::ALL[current - 1]
            };

            if self.deep_learn_page == DeepLearnPage::Chat {
                self.ensure_agent_chat_greeting();
            }
        }
        KeyCode::F(2)
        | KeyCode::Char('r')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            self.run_agent_from_chat()?;
        }
        KeyCode::PageUp | KeyCode::Up => {
            self.agent_chat_scroll =
                self.agent_chat_scroll.saturating_add(5);
        }
        KeyCode::PageDown | KeyCode::Down => {
            self.agent_chat_scroll =
                self.agent_chat_scroll.saturating_sub(5);
        }
        KeyCode::Enter
            if key.modifiers.contains(KeyModifiers::SHIFT) =>
        {
            self.agent_chat_input.push('\n');
        }
        KeyCode::Enter => {
            self.agent_chat_scroll = 0;
            self.handle_agent_chat_submit()?;
        }
        KeyCode::Backspace => {
            self.agent_chat_input.pop();
        }
        KeyCode::Char(ch) => {
            self.agent_chat_input.push(ch);
        }
        _ => {}
    }

    Ok(())
}


fn start_deep_learn(&mut self) -> Result<()> {
    if self.deep_learn_rx.is_some() {
        self.status =
            "AI Agent already has a research/simulation job running.".to_string();
        return Ok(());
    }

    let Some(deck) = self
        .deep_agent_result
        .as_ref()
        .map(|result| result.best.deck.clone())
        .or_else(|| self.parsed.clone())
    else {
        return Ok(());
    };

    let bracket = self
        .analysis
        .as_ref()
        .map(|analysis| analysis.bracket.bracket)
        .unwrap_or(3);

    let (tx, rx) = mpsc::channel::<DeepLearnEvent>();

    self.deep_learn_rx = Some(rx);
    self.deep_learn_trace.clear();
    push_trace_log(
        &mut self.deep_learn_trace,
        "DEEP",
        format!(
            "START RESEARCH • deck={} • bracket=B{} • commanders={}",
            deck.name,
            bracket,
            deck.commanders
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>()
                .join(" + "),
        ),
    );
    self.deep_learn_status =
        "Researching commander shell and current list...".to_string();

    thread::spawn(move || {
        let result = (|| -> Result<DeepLearnReport> {
            let db = CardDb::open("./data/cards.sqlite")?;

            deep_learn_deck(
                &deck,
                &db,
                bracket,
                |status| {
                    let _ = tx.send(
                        DeepLearnEvent::ResearchProgress(status)
                    );
                },
            )
        })();

        let _ = tx.send(DeepLearnEvent::ResearchFinished(
            result.map_err(|err| format!("{err:#}")),
        ));
    });

    Ok(())
}

fn start_agent_requested_simulation(
    &mut self,
    intensity: TestingIntensity,
    exact_runs: Option<u32>,
    meta_matchups: bool,
) -> Result<()> {
    if self.deep_learn_rx.is_some()
        || self.deep_agent_rx.is_some()
        || self.generation_rx.is_some()
        || self.agent_chat_rx.is_some()
    {
        self.agent_chat_status =
            "Wait for the current AI task to finish.".to_string();
        return Ok(());
    }

    let Some(deck) = self
        .deep_agent_result
        .as_ref()
        .map(|result| result.best.deck.clone())
        .or_else(|| self.parsed.clone())
    else {
        return Ok(());
    };

    self.deep_learn_intensity = intensity;

    let (tx, rx) = mpsc::channel::<DeepLearnEvent>();
    self.deep_learn_rx = Some(rx);

    let run_label = exact_runs
        .map(|runs| format!("{runs} exact runs"))
        .unwrap_or_else(|| {
            format!("{} testing", intensity.label())
        });

    self.deep_learn_status =
        format!("Running {run_label}...");
    self.agent_chat_status =
        format!("Running {run_label} on the current deck.");

    push_trace_log(
        &mut self.deep_learn_trace,
        "SIM",
        format!(
            "CHAT REQUEST • testing={} • exact_runs={}",
            intensity.label(),
            exact_runs
                .map(|runs| runs.to_string())
                .unwrap_or_else(|| "none".to_string()),
        ),
    );

    thread::spawn(move || {
        if meta_matchups {
            let result = (|| -> Result<MetaMatchupSummary> {
                let db = CardDb::open("./data/cards.sqlite")?;
                let mut references =
                    load_cached_meta_reference_decks(16);

                if references.len() < 3 {
                    let commanders = deck
                        .commanders
                        .iter()
                        .map(|entry| entry.name.clone())
                        .collect::<Vec<_>>();

                    if !commanders.is_empty() {
                        let snapshot =
                            gather_reference_decks(
                                &commanders,
                                5,
                            );

                        references.extend(snapshot.decks);

                        let mut seen =
                            std::collections::HashSet::<String>::new();

                        references.retain(|reference| {
                            let key = if reference.url.trim().is_empty() {
                                format!(
                                    "{}::{}",
                                    reference.source,
                                    reference.title,
                                )
                            } else {
                                reference.url.to_ascii_lowercase()
                            };

                            seen.insert(key)
                        });

                        references.truncate(16);
                    }
                }

                let runs = exact_runs
                    .unwrap_or_else(|| intensity.goldfish_runs());

                simulate_meta_matchups(
                    &deck,
                    &db,
                    &references,
                    runs,
                    |status| {
                        let _ = tx.send(
                            DeepLearnEvent::MetaSimulationProgress(
                                status,
                            ),
                        );
                    },
                )
            })();

            let _ = tx.send(
                DeepLearnEvent::MetaSimulationFinished(
                    result.map_err(|err| format!("{err:#}")),
                ),
            );
        } else {
            let result = (|| -> Result<DeepLearnTestSummary> {
                let db = CardDb::open("./data/cards.sqlite")?;

                simulate_deep_learn_games_with_runs(
                    &deck,
                    &db,
                    intensity,
                    exact_runs,
                    |status| {
                        let _ = tx.send(
                            DeepLearnEvent::SimulationProgress(
                                status,
                            ),
                        );
                    },
                )
            })();

            let _ = tx.send(
                DeepLearnEvent::SimulationFinished(
                    result.map_err(|err| format!("{err:#}")),
                ),
            );
        }
    });

    Ok(())
}

fn start_deep_learn_simulation(&mut self) -> Result<()> {
    if self.deep_learn_rx.is_some() {
        self.status =
            "AI Agent already has a research/simulation job running.".to_string();
        return Ok(());
    }

    let Some(deck) = self.parsed.clone() else {
        return Ok(());
    };

    let intensity = self.deep_learn_intensity;
    let (tx, rx) = mpsc::channel::<DeepLearnEvent>();

    self.deep_learn_rx = Some(rx);
    self.deep_learn_status = format!(
        "Running {} game simulation...",
        intensity.label(),
    );
    push_trace_log(
        &mut self.deep_learn_trace,
        "SIM",
        format!(
            "START • intensity={} • draw_runs={} • goldfish_runs={}",
            intensity.label(),
            intensity.draw_runs(),
            intensity.goldfish_runs(),
        ),
    );

    thread::spawn(move || {
        let result = (|| -> Result<DeepLearnTestSummary> {
            let db = CardDb::open("./data/cards.sqlite")?;

            simulate_deep_learn_games(
                &deck,
                &db,
                intensity,
                |status| {
                    let _ = tx.send(
                        DeepLearnEvent::SimulationProgress(status)
                    );
                },
            )
        })();

        let _ = tx.send(DeepLearnEvent::SimulationFinished(
            result.map_err(|err| format!("{err:#}")),
        ));
    });

    Ok(())
}

fn poll_deep_learn_events(&mut self) -> Result<()> {
    enum Finished {
        Research(Result<DeepLearnReport, String>),
        Simulation(Result<DeepLearnTestSummary, String>),
        MetaSimulation(Result<MetaMatchupSummary, String>),
    }

    let mut finished = None;

    if let Some(rx) = &self.deep_learn_rx {
        loop {
            match rx.try_recv() {
                Ok(DeepLearnEvent::ResearchProgress(status)) => {
                    self.deep_learn_status = status.clone();
                    push_trace_log(&mut self.deep_learn_trace, "DEEP", &status);
                    self.status = format!("AI Agent • {status}");
                }

                Ok(DeepLearnEvent::SimulationProgress(status)) => {
                    self.deep_learn_status = status.clone();
                    push_trace_log(&mut self.deep_learn_trace, "SIM", &status);
                    self.status = format!("AI Agent • {status}");
                }

                Ok(DeepLearnEvent::MetaSimulationProgress(status)) => {
                    self.deep_learn_status = status.clone();
                    push_trace_log(
                        &mut self.deep_learn_trace,
                        "META",
                        &status,
                    );
                    self.status = format!("AI Agent • {status}");
                }

                Ok(DeepLearnEvent::ResearchFinished(result)) => {
                    finished = Some(Finished::Research(result));
                    break;
                }

                Ok(DeepLearnEvent::SimulationFinished(result)) => {
                    finished = Some(Finished::Simulation(result));
                    break;
                }

                Ok(DeepLearnEvent::MetaSimulationFinished(result)) => {
                    finished = Some(Finished::MetaSimulation(result));
                    break;
                }

                Err(TryRecvError::Empty) => break,

                Err(TryRecvError::Disconnected) => {
                    self.deep_learn_rx = None;
                    self.deep_learn_status =
                        "AI Agent worker disconnected unexpectedly."
                            .to_string();
                    break;
                }
            }
        }
    }

    if let Some(finished) = finished {
        self.deep_learn_rx = None;

        match finished {
            Finished::Research(result) => match result {
                Ok(mut report) => {
                    let previous_confidence = self
                        .deep_learn
                        .as_ref()
                        .map(|existing| existing.confidence)
                        .unwrap_or(0);

                    let additional_evidence = self
                        .deck_sources
                        .iter()
                        .filter(|source| {
                            !report.sources.iter().any(|existing| {
                                existing.url.eq_ignore_ascii_case(
                                    &source.url,
                                )
                            })
                        })
                        .count();

                    if additional_evidence > 0 {
                        let evidence_boost =
                            (additional_evidence.min(10) * 2) as u8;

                        report.confidence = report
                            .confidence
                            .saturating_add(evidence_boost)
                            .min(100);

                        report.findings.push(format!(
                            "Additional Agent/user evidence: {} unique source(s) increased the evidence-weighted confidence to {}%.",
                            additional_evidence,
                            report.confidence,
                        ));
                    }

                    push_trace_log(
                        &mut self.deep_learn_trace,
                        "DEEP",
                        format!(
                            "SUCCESS • references={} • edhrec_signals={} • confidence={} • adds={} • cuts={} • reference_combos={}",
                            report.reference_decks,
                            report.edhrec_signals,
                            report.confidence,
                            report.adds.len(),
                            report.cuts.len(),
                            report.reference_combos.len(),
                        ),
                    );
                    self.deep_learn_status = format!(
                        "Research complete • {} EDHREC signals • {} references • confidence {}%.",
                        report.edhrec_signals,
                        report.reference_decks,
                        report.confidence,
                    );

                    for line in &report.decision_trace {
                        push_trace_log(
                            &mut self.deep_learn_trace,
                            "DEEP-DECISION",
                            line,
                        );
                    }

                    for source in &report.sources {
                        if !self.deck_sources.iter().any(|existing| {
                            existing.url.eq_ignore_ascii_case(&source.url)
                                && existing.title.eq_ignore_ascii_case(&source.title)
                        }) {
                            self.deck_sources.push(source.clone());
                        }
                    }

                    self.cache_deck_run(
                        "DEEP RESEARCH",
                        format!(
                            "{} references • {} EDHREC signals • {}% confidence • {} add(s) • {} cut(s)",
                            report.reference_decks,
                            report.edhrec_signals,
                            report.confidence,
                            report.adds.len(),
                            report.cuts.len(),
                        ),
                        report.decision_trace.clone(),
                    );

                    let new_confidence = report.confidence;

                    self.deep_learn = Some(report);
                    self.deep_learn_selected = 0;
                    let _ = self.persist_current_sources();
                    self.status =
                        "AI Agent research complete.".to_string();

                    if self.deep_learn_page == DeepLearnPage::Chat {
                        self.agent_chat_messages.push(
                            AgentChatMessage {
                                role: "system".to_string(),
                                content: format!(
                                    "Research complete • confidence {}% → {}%.{}",
                                    previous_confidence,
                                    new_confidence,
                                    if new_confidence
                                        <= previous_confidence
                                    {
                                        " No new independent evidence increased confidence this pass."
                                    } else {
                                        ""
                                    },
                                ),
                            },
                        );
                        self.agent_chat_scroll = 0;
                    }

                    self.persist_ai_session();

                    if self.agent_chat_optimize_after_research {
                        self.agent_chat_optimize_after_research = false;
                        self.agent_chat_status =
                            "Research complete. Applying the requested deck changes."
                                .to_string();
                        self.run_agent_from_chat()?;
                    } else if self.agent_chat_simulate_after_research {
                        self.agent_chat_simulate_after_research = false;
                        self.agent_chat_status =
                            "Research complete. Running the requested playtest."
                                .to_string();
                        self.run_pending_agent_simulation()?;
                    } else if self.deep_learn_page
                        == DeepLearnPage::Chat
                    {
                        self.agent_chat_status =
                            format!(
                                "Research updated • confidence {}%.",
                                new_confidence,
                            );

                        if let Some(instruction) =
                            self.agent_chat_follow_up.take()
                        {
                            self.start_agent_chat_with_message(
                                instruction,
                            )?;
                        }
                    }
                }

                Err(err) => {
                    push_trace_log(
                        &mut self.deep_learn_trace,
                        "DEEP",
                        format!("FAILED • {err}"),
                    );
                    self.deep_learn_status =
                        format!("AI Agent research failed: {err}");
                    self.status = self.deep_learn_status.clone();
                }
            },

            Finished::Simulation(result) => match result {
                Ok(testing) => {
                    push_trace_log(
                        &mut self.deep_learn_trace,
                        "SIM",
                        format!(
                            "SUCCESS • games={} • earliest={} • likely={} • median={} • by_t4={:.2}% • by_t5={:.2}% • by_t6={:.2}%",
                            testing.goldfish.runs,
                            testing.goldfish
                                .earliest_combo_turn
                                .map(|turn| format!("T{turn}"))
                                .unwrap_or_else(|| "none".to_string()),
                            testing.goldfish
                                .most_likely_combo_turn
                                .map(|turn| format!("T{turn}"))
                                .unwrap_or_else(|| "none".to_string()),
                            testing.goldfish
                                .median_combo_turn
                                .map(|turn| format!("T{turn}"))
                                .unwrap_or_else(|| "none".to_string()),
                            testing.goldfish.combo_by_t4_pct,
                            testing.goldfish.combo_by_t5_pct,
                            testing.goldfish.combo_by_t6_pct,
                        ),
                    );
                    self.deep_learn_status = format!(
                        "{} simulation complete • {} games.",
                        testing.intensity.label(),
                        testing.goldfish.runs,
                    );

                    if let Some(report) = self.deep_learn.as_mut() {
                        report.testing = Some(testing.clone());
                    }

                    self.cache_deck_run(
                        "DEEP SIM",
                        format!(
                            "{} • {} games • earliest {} • likely {} • T5 {:.2}%",
                            testing.intensity.label(),
                            testing.goldfish.runs,
                            testing.goldfish.earliest_combo_turn
                                .map(|turn| format!("T{turn}"))
                                .unwrap_or_else(|| "none".to_string()),
                            testing.goldfish.most_likely_combo_turn
                                .map(|turn| format!("T{turn}"))
                                .unwrap_or_else(|| "none".to_string()),
                            testing.goldfish.combo_by_t5_pct,
                        ),
                        self.deep_learn_trace.clone(),
                    );

                    let chat_summary = format!(
                        "Simulation complete • {} games • earliest {} • likely {} • T4 {:.2}% • T5 {:.2}% • T6 {:.2}%.",
                        testing.goldfish.runs,
                        testing.goldfish
                            .earliest_combo_turn
                            .map(|turn| format!("T{turn}"))
                            .unwrap_or_else(|| "none by T8".to_string()),
                        testing.goldfish
                            .most_likely_combo_turn
                            .map(|turn| format!("T{turn}"))
                            .unwrap_or_else(|| "none".to_string()),
                        testing.goldfish.combo_by_t4_pct,
                        testing.goldfish.combo_by_t5_pct,
                        testing.goldfish.combo_by_t6_pct,
                    );

                    self.deep_learn_testing = Some(testing);

                    if self.deep_learn_page == DeepLearnPage::Chat {
                        self.agent_chat_messages.push(
                            AgentChatMessage {
                                role: "system".to_string(),
                                content: chat_summary,
                            },
                        );
                        self.agent_chat_status =
                            "Playtest complete.".to_string();
                        self.agent_chat_scroll = 0;
                    } else {
                        self.deep_learn_page =
                            DeepLearnPage::WinSpeed;
                    }

                    self.persist_ai_session();
                    self.status =
                        "AI Agent game simulation complete.".to_string();

                    if self.agent_chat_optimize_after_simulation {
                        self.agent_chat_optimize_after_simulation = false;
                        self.agent_chat_status =
                            "Playtest complete. Applying the requested deck changes next."
                                .to_string();
                        self.run_agent_from_chat()?;
                    } else if let Some(instruction) =
                        self.agent_chat_follow_up.take()
                    {
                        self.start_agent_chat_with_message(
                            instruction,
                        )?;
                    }
                }

                Err(err) => {
                    push_trace_log(
                        &mut self.deep_learn_trace,
                        "SIM",
                        format!("FAILED • {err}"),
                    );
                    self.deep_learn_status =
                        format!("AI Agent simulation failed: {err}");
                    self.status = self.deep_learn_status.clone();
                }
            },
            Finished::MetaSimulation(result) => match result {
                Ok(summary) => {
                    push_trace_log(
                        &mut self.deep_learn_trace,
                        "META",
                        format!(
                            "SUCCESS • games={} • opponents={} • outright_win={:.2}% • equity={:.2}% • shared_first={:.2}% • loss={:.2}%",
                            summary.runs,
                            summary.opponents_used,
                            summary.outright_win_pct,
                            summary.equity_win_pct,
                            summary.shared_first_pct,
                            summary.loss_pct,
                        ),
                    );

                    self.cache_deck_run(
                        "META SIM",
                        format!(
                            "{} games • {} opponents • outright {:.2}% • equity {:.2}% • loss {:.2}%",
                            summary.runs,
                            summary.opponents_used,
                            summary.outright_win_pct,
                            summary.equity_win_pct,
                            summary.loss_pct,
                        ),
                        self.deep_learn_trace.clone(),
                    );

                    let mut matchup_lines = summary
                        .opponents
                        .iter()
                        .take(8)
                        .map(|opponent| {
                            format!(
                                "{}: {:.1}% H2H",
                                opponent.title,
                                opponent.head_to_head_win_pct,
                            )
                        })
                        .collect::<Vec<_>>();

                    if matchup_lines.is_empty() {
                        matchup_lines.push(
                            "No opponent breakdown available."
                                .to_string(),
                        );
                    }

                    let chat_summary = format!(
                        "Meta test complete • {} pod games • {} opponent decks • outright win {:.2}% • equity win {:.2}% • shared first {:.2}% • loss {:.2}% • average winning turn {}.\n{}",
                        summary.runs,
                        summary.opponents_used,
                        summary.outright_win_pct,
                        summary.equity_win_pct,
                        summary.shared_first_pct,
                        summary.loss_pct,
                        summary
                            .average_winning_turn
                            .map(|turn| format!("{turn:.2}"))
                            .unwrap_or_else(|| "n/a".to_string()),
                        matchup_lines.join("\n"),
                    );

                    self.deep_learn_status = format!(
                        "Meta matchup test complete • {} games • {:.2}% equity.",
                        summary.runs,
                        summary.equity_win_pct,
                    );
                    self.meta_matchup = Some(summary);

                    if self.deep_learn_page == DeepLearnPage::Chat {
                        self.agent_chat_messages.push(
                            AgentChatMessage {
                                role: "system".to_string(),
                                content: chat_summary,
                            },
                        );
                        self.agent_chat_status =
                            "Meta matchup test complete. AI Optimization was not run."
                                .to_string();
                        self.agent_chat_scroll = 0;
                    }

                    self.persist_ai_session();
                    self.status =
                        "AI Agent meta matchup simulation complete."
                            .to_string();
                }

                Err(err) => {
                    push_trace_log(
                        &mut self.deep_learn_trace,
                        "META",
                        format!("FAILED • {err}"),
                    );
                    self.deep_learn_status =
                        format!("Meta matchup simulation failed: {err}");
                    self.agent_chat_status =
                        self.deep_learn_status.clone();

                    if self.deep_learn_page == DeepLearnPage::Chat {
                        self.agent_chat_messages.push(
                            AgentChatMessage {
                                role: "system".to_string(),
                                content: self.deep_learn_status.clone(),
                            },
                        );
                    }

                    self.persist_ai_session();
                    self.status = self.deep_learn_status.clone();
                }
            },

        }
    }

    Ok(())
}

pub fn deep_learn_iteration_preview_names(&self) -> Vec<String> {
    self.deep_agent_result
        .as_ref()
        .map(|result| {
            result
                .top_iterations
                .iter()
                .filter_map(|iteration| {
                    iteration
                        .deck
                        .deck
                        .commanders
                        .first()
                        .or_else(|| iteration.deck.deck.main.first())
                        .map(|entry| entry.name.clone())
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn next_deep_learn_focus(&mut self) {
    let count = self
        .deep_agent_result
        .as_ref()
        .map(|result| result.top_iterations.len())
        .unwrap_or(0);

    if count > 0 {
        self.deep_learn_selected =
            (self.deep_learn_selected + 1).min(count - 1);
    }

    self.preview_deep_learn_card();
}

fn preview_deep_learn_card(&mut self) {
    if let Some(name) = self
        .deep_learn_iteration_preview_names()
        .get(self.deep_learn_selected)
        .cloned()
    {
        self.select_card_by_name(&name);
    }
}

    fn next_page(&mut self) {
        let current = Page::ALL
            .iter()
            .position(|page| *page == self.page)
            .unwrap_or(0);

        self.page = Page::ALL[(current + 1) % Page::ALL.len()];
    }

    fn prev_page(&mut self) {
        let current = Page::ALL
            .iter()
            .position(|page| *page == self.page)
            .unwrap_or(0);

        self.page = if current == 0 {
            Page::ALL[Page::ALL.len() - 1]
        } else {
            Page::ALL[current - 1]
        };
    }

    pub fn card_count(&self) -> usize {
        self.card_entries().len()
    }

    pub fn card_entries(&self) -> Vec<(String, u32, bool)> {
        let Some(deck) = self.parsed.as_ref() else {
            return Vec::new();
        };

        let mut entries = Vec::new();

        match self.card_zone {
            CardZone::Mainboard => {
                for entry in &deck.commanders {
                    entries.push((entry.name.clone(), entry.quantity, true));
                }

                for entry in &deck.main {
                    entries.push((entry.name.clone(), entry.quantity, false));
                }
            }

            CardZone::Sideboard => {
                for entry in &deck.sideboard {
                    entries.push((entry.name.clone(), entry.quantity, false));
                }
            }
        }

        entries
    }

    pub fn sideboard_count(&self) -> usize {
        self.parsed
            .as_ref()
            .map(|deck| {
                deck.sideboard
                    .iter()
                    .map(|entry| entry.quantity as usize)
                    .sum()
            })
            .unwrap_or(0)
    }

    fn selected_card_name(&self) -> Option<String> {
        self.card_entries()
            .get(self.selected_card)
            .map(|(name, _, _)| name.clone())
    }

    fn next_card(&mut self) {
        let count = self.card_count();

        if count == 0 {
            return;
        }

        self.selected_card = (self.selected_card + 1).min(count - 1);
        self.ensure_card_visible();
        self.refresh_card_preview();
    }

    fn prev_card(&mut self) {
        if self.selected_card == 0 {
            return;
        }

        self.selected_card -= 1;
        self.ensure_card_visible();
        self.refresh_card_preview();
    }

    fn toggle_card_zone(&mut self) {
        let next = match self.card_zone {
            CardZone::Mainboard => CardZone::Sideboard,
            CardZone::Sideboard => CardZone::Mainboard,
        };

        self.set_card_zone(next);
    }

    fn set_card_zone(&mut self, zone: CardZone) {
        self.card_zone = zone;
        self.selected_card = 0;
        self.card_scroll = 0;
        self.refresh_card_preview();
    }

    fn ensure_card_visible(&mut self) {
        let height = self
            .regions
            .card_list
            .map(|area| area.height.saturating_sub(2) as usize)
            .unwrap_or(20)
            .max(1);

        if self.selected_card < self.card_scroll {
            self.card_scroll = self.selected_card;
        } else if self.selected_card >= self.card_scroll + height {
            self.card_scroll = self.selected_card + 1 - height;
        }
    }

    pub fn select_card_by_name(&mut self, name: &str) {
        match self.db.get_card(name) {
            Ok(Some(card)) => {
                let image_result = self.load_card_image(&card);
                self.selected_card_info = Some(card);

                if let Err(err) = image_result {
                    self.status =
                        format!("Card selected, but preview image failed: {err}");
                }
            }

            Ok(None) => {
                self.selected_card_info = None;
                self.card_image = None;
                self.status =
                    format!("Card '{}' is not in the local Scryfall database.", name);
            }

            Err(err) => {
                self.selected_card_info = None;
                self.card_image = None;
                self.status = format!("Could not load card '{}': {err}", name);
            }
        }
    }

    fn set_safe_image_mode(&mut self, enabled: bool) {
    if self.safe_image_mode == enabled {
        return;
    }

    self.safe_image_mode = enabled;

    // Halfblocks avoid stale graphics-plane artifacts during rapid redraws.
    self.refresh_card_preview();
}

pub fn is_ai_optimization_running(&self) -> bool {
    self.deep_agent_rx.is_some()
}

pub fn runtime_worker_label(&self) -> &'static str {
    if self.deep_agent_rx.is_some() {
        "AI OPT"
    } else if self.deep_learn_rx.is_some() {
        "AI TEST"
    } else if self.generation_rx.is_some() {
        "BUILD"
    } else if self.agent_chat_rx.is_some() {
        "AI CHAT"
    } else {
        "IDLE"
    }
}

fn background_worker_active(&self) -> bool {
    self.deep_agent_rx.is_some()
        || self.deep_learn_rx.is_some()
        || self.generation_rx.is_some()
        || self.agent_chat_rx.is_some()
}

fn refresh_card_preview(&mut self) {
        self.card_image = None;
        self.selected_card_info = None;

        let Some(name) = self.selected_card_name() else {
            return;
        };

        self.select_card_by_name(&name);
    }

    fn load_card_image(&mut self, card: &Card) -> Result<()> {
        if card.image_uri.trim().is_empty() {
            return Err(anyhow!(
                "No image URL is stored. Press UPDATE to refresh Scryfall data."
            ));
        }

        let cache_path =
            PathBuf::from("./data/images").join(format!("{}.jpg", card.oracle_id));

        if !cache_path.exists() {
            download_image(&card.image_uri, &cache_path)
                .with_context(|| format!("Could not download image for {}", card.name))?;
        }

        let image = ImageReader::open(&cache_path)
            .with_context(|| format!("Could not open {}", cache_path.display()))?
            .decode()
            .context("Could not decode card image")?;

        let protocol = if self.safe_image_mode {
            let safe_picker = Picker::halfblocks();

            safe_picker
                .new_protocol(
                    image,
                    Size::new(32, 24),
                    Resize::Fit(None),
                )
                .context(
                    "Could not prepare safe terminal card image",
                )?
        } else {
            self.image_picker
                .new_protocol(
                    image,
                    Size::new(32, 24),
                    Resize::Fit(None),
                )
                .context(
                    "Could not prepare image for terminal rendering",
                )?
        };

        self.card_image = Some(protocol);
        Ok(())
    }

    fn open_store(&mut self, store: Store) {
        let Some(card) = self.selected_card_info.as_ref() else {
            self.status = "Select a card first.".to_string();
            return;
        };

        let card_name = card.name.clone();

        let uri = match store {
            Store::Tcgplayer => card.tcgplayer_uri.clone(),
            Store::Cardmarket => card.cardmarket_uri.clone(),
            Store::Cardhoarder => card.cardhoarder_uri.clone(),
        };

        if uri.trim().is_empty() {
            self.status = format!(
                "No {} purchase link is available for {}.",
                store.name(),
                card_name
            );
            return;
        }

        match open::that(&uri) {
            Ok(_) => {
                self.status =
                    format!("Opened {} for {}.", store.name(), card_name);
            }
            Err(err) => {
                self.status =
                    format!("Could not open {} link: {err}", store.name());
            }
        }
    }

fn sideboard_selected_suggestion(&mut self) -> Result<()> {
    let Some(selection) = self.selected_suggestion.clone() else {
        self.status = "Click a CUT or ADD suggestion first.".to_string();
        return Ok(());
    };

    let Some(mut deck) = self.parsed.clone() else {
        return Ok(());
    };

    match selection.kind {
        SuggestionKind::Cut => {
            let Some(index) = deck
                .main
                .iter()
                .position(|entry| entry.name.eq_ignore_ascii_case(&selection.card))
            else {
                self.status = format!("{} is no longer in main.", selection.card);
                return Ok(());
            };

            if deck.main[index].quantity > 1 {
                deck.main[index].quantity -= 1;
            } else {
                deck.main.remove(index);
            }

            add_to_sideboard(&mut deck, &selection.card);
        }
        SuggestionKind::Add => {
            add_to_sideboard(&mut deck, &selection.card);
        }
    }

    let Some(file) = self.decks.get(self.selected_deck) else {
        self.status = "Could not determine the selected deck file.".to_string();
        return Ok(());
    };

    write_deck_file(&file.path, &deck)?;
    self.analysis = Some(analyze_deck(&deck, &self.db)?);
    self.parsed = Some(deck);
    self.selected_suggestion = None;

    self.status = match selection.kind {
        SuggestionKind::Cut => format!(
            "Moved {} from main to sideboard and saved.",
            selection.card
        ),
        SuggestionKind::Add => format!(
            "Added {} to sideboard and saved.",
            selection.card
        ),
    };

    Ok(())
}


fn select_archetype_tag(&mut self, tag: &str) {
    self.selected_archetype_tag = Some(tag.to_string());

    self.deep_agent_goal = if tag
        .trim()
        .to_ascii_lowercase()
        .contains("stax")
    {
        OptimizationGoal::StaxLock
    } else {
        OptimizationGoal::FastWin
    };

    let Some(deck) = self.parsed.as_ref() else {
        return;
    };

    for entry in deck.commanders.iter().chain(deck.main.iter()) {
        let Ok(Some(card)) = self.db.get_card(&entry.name) else {
            continue;
        };

        if tag_names_for_card(&card).iter().any(|name| name == tag) {
            let card_name = card.name.clone();
            self.select_card_by_name(&card_name);
            self.status =
                format!("{tag}: previewing {card_name}.");
            return;
        }
    }

    self.status = format!("No matching card found for {tag}.");
}

pub fn cards_for_archetype(&self, tag: &str) -> Vec<String> {
    let Some(deck) = self.parsed.as_ref() else {
        return Vec::new();
    };

    let mut cards = Vec::new();

    for entry in deck.commanders.iter().chain(deck.main.iter()) {
        let Ok(Some(card)) = self.db.get_card(&entry.name) else {
            continue;
        };

        if tag_names_for_card(&card).iter().any(|name| name == tag)
            && !cards.iter().any(|existing| existing == &card.name)
        {
            cards.push(card.name);
        }
    }

    cards
}

}

fn add_to_sideboard(deck: &mut ParsedDeck, name: &str) {
    if deck
        .sideboard
        .iter()
        .any(|entry| entry.name.eq_ignore_ascii_case(name))
    {
        return;
    }

    deck.sideboard.push(crate::deck::DeckEntry {
        quantity: 1,
        name: name.to_string(),
    });
}

#[derive(Debug, Clone, Copy)]
enum Store {
    Tcgplayer,
    Cardmarket,
    Cardhoarder,
}

impl Store {
    fn name(self) -> &'static str {
        match self {
            Store::Tcgplayer => "TCGplayer",
            Store::Cardmarket => "Cardmarket",
            Store::Cardhoarder => "Cardhoarder",
        }
    }
}

fn download_image(uri: &str, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let bytes = Client::builder()
        .build()?
        .get(uri)
        .header(USER_AGENT, IMAGE_USER_AGENT)
        .header(ACCEPT, "image/avif,image/webp,image/*,*/*;q=0.8")
        .send()?
        .error_for_status()?
        .bytes()?;

    std::fs::write(path, bytes)?;
    Ok(())
}

fn point_in(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x
        && x < area.x.saturating_add(area.width)
        && y >= area.y
        && y < area.y.saturating_add(area.height)
}
