//! 対話モードの進行。

use std::io::{BufRead, Write};

use crate::core::config::quota::Metric;
use crate::core::errors::{AppError, ErrorCode};
use crate::core::select::{self, Ref};
use crate::core::session::{DocumentRef, SessionState};
use crate::ui::menu::{self, Input, Screen};
use crate::ui::render::Renderer;
use crate::usecases::{App, document, history as history_uc, search as search_uc};

/// 対話モードの実行。
pub struct Interactive {
    app: App,
    renderer: Renderer,
    screen: Screen,
    session: SessionState,
    query: String,
    /// 現在の表示対象文書（選択順）。
    documents: Vec<DocumentRef>,
    /// 直近のアウトライン深度。
    depth: u8,
    /// 連続した不正入力の回数。
    invalid: u32,
}

impl Interactive {
    pub fn new(app: App, renderer: Renderer) -> Self {
        let depth = app.settings.context.outline_depth;
        renderer.set_secrets(crate::core::secret::redaction_list_for_engines(
            &app.secrets,
            &app.engines,
        ));
        Self {
            app,
            renderer,
            screen: Screen::Query,
            session: SessionState::default(),
            query: String::new(),
            documents: Vec::new(),
            depth,
            invalid: 0,
        }
    }

    /// 起動（`stdin` から 1 行ずつ読む）。
    pub fn run(&mut self, input: &mut dyn BufRead, out: &mut dyn Write) -> Result<(), AppError> {
        let id = self.app.ensure_session()?;
        if let Some(session) = self.app.sessions.load(&id)? {
            self.session = session;
            if !self.session.last_query.is_empty() {
                self.query = self.session.last_query.clone();
            }
        } else {
            self.session = SessionState::new(&id, self.app.clock.now_utc());
            self.app.sessions.save(&self.session)?;
        }
        self.app.set_session(&id);
        if !self.app.quiet {
            writeln!(out, "session {id} (resume with --session {id})").map_err(io_error)?;
        }

        loop {
            match self.screen {
                Screen::Query => {
                    write!(out, "query: ").map_err(io_error)?;
                    out.flush().map_err(io_error)?;
                    let Some(line) = read_line(input)? else {
                        return Ok(());
                    };
                    match menu::parse_input(Screen::Query, &line) {
                        Input::Query(q) => {
                            self.query = q;
                            self.run_search(out)?;
                        }
                        Input::Empty => continue,
                        Input::Command(c) if c == "quit" => return Ok(()),
                        _ => self.show_help(out)?,
                    }
                }
                Screen::Results => {
                    self.show_results(out)?;
                    write!(
                        out,
                        "{}",
                        menu::results_menu(
                            self.session.next_result_id(),
                            !self.session.page.exhausted,
                        )
                    )
                    .map_err(io_error)?;
                    out.flush().map_err(io_error)?;
                    let Some(line) = read_line(input)? else {
                        return Ok(());
                    };
                    let mut next: Option<Screen> = None;
                    match menu::parse_input(Screen::Results, &line) {
                        Input::Choice(1) => return Ok(()),
                        Input::Choice(2) => next = Some(Screen::SelectResults),
                        Input::Choice(3) => self.show_next(out)?,
                        Input::Choice(4) => next = Some(Screen::Query),
                        Input::Command(c) if c == "quit" => return Ok(()),
                        Input::Command(c) if c == "next" => self.show_next(out)?,
                        Input::Command(c) if c == "refine" => next = Some(Screen::Query),
                        Input::Command(c) if c == "history" => self.show_history(out)?,
                        Input::Empty => continue,
                        _ => {
                            self.invalid(out)?;
                        }
                    }
                    if let Some(screen) = next {
                        self.screen = screen;
                    }
                }
                Screen::SelectResults => {
                    write!(out, "selection (e.g. 1,3 / 1-3 / R1): ").map_err(io_error)?;
                    out.flush().map_err(io_error)?;
                    let Some(line) = read_line(input)? else {
                        return Ok(());
                    };
                    match menu::parse_input(Screen::SelectResults, &line) {
                        Input::Selection(selections) => {
                            self.fetch_selected(out, &selections)?;
                            if self.documents.is_empty() {
                                self.screen = Screen::Results;
                            } else {
                                self.screen = Screen::Outline;
                            }
                        }
                        Input::Empty => self.screen = Screen::Results,
                        Input::Command(c) if c == "back" => self.screen = Screen::Results,
                        Input::Command(c) if c == "quit" => return Ok(()),
                        Input::Command(c) if c == "history" => self.show_history(out)?,
                        _ => {
                            if self.invalid(out)? {
                                self.screen = Screen::Results;
                            }
                        }
                    }
                }
                Screen::Outline => {
                    self.show_outline(out)?;
                    write!(out, "{}", menu::outline_menu()).map_err(io_error)?;
                    out.flush().map_err(io_error)?;
                    let Some(line) = read_line(input)? else {
                        return Ok(());
                    };
                    let mut next: Option<Screen> = None;
                    match menu::parse_input(Screen::Outline, &line) {
                        Input::Choice(1) => return Ok(()),
                        Input::Choice(2) => next = Some(Screen::SelectSections),
                        Input::Choice(3) => {
                            write!(out, "depth (2-4): ").map_err(io_error)?;
                            out.flush().map_err(io_error)?;
                            if let Some(depth_line) = read_line(input)? {
                                match menu::parse_input(Screen::Outline, &depth_line) {
                                    Input::Choice(n) if (2..=4).contains(&n) => {
                                        self.depth = n;
                                        self.app.settings.context.outline_depth = n;
                                    }
                                    _ => {
                                        writeln!(out, "enter a number between 2 and 4")
                                            .map_err(io_error)?;
                                    }
                                }
                            }
                        }
                        Input::Choice(4) => next = Some(Screen::Results),
                        Input::Choice(5) => self.show_history(out)?,
                        Input::Command(c) if c == "quit" => return Ok(()),
                        Input::Command(c) if c == "back" => next = Some(Screen::Results),
                        Input::Command(c) if c == "history" => self.show_history(out)?,
                        Input::Empty => continue,
                        _ => {
                            self.invalid(out)?;
                        }
                    }
                    if let Some(screen) = next {
                        self.screen = screen;
                    }
                }
                Screen::SelectSections => {
                    write!(out, "section (e.g. D1:S3 / S3): ").map_err(io_error)?;
                    out.flush().map_err(io_error)?;
                    let Some(line) = read_line(input)? else {
                        return Ok(());
                    };
                    match menu::parse_input(Screen::SelectSections, &line) {
                        Input::Selection(selections) => {
                            self.read_sections(out, &selections)?;
                            self.screen = Screen::Outline;
                        }
                        Input::Empty => self.screen = Screen::Outline,
                        Input::Command(c) if c == "back" => self.screen = Screen::Outline,
                        Input::Command(c) if c == "quit" => return Ok(()),
                        _ => {
                            self.invalid(out)?;
                        }
                    }
                }
                Screen::History => {
                    self.show_history(out)?;
                    self.screen = Screen::Results;
                }
                Screen::Exit => return Ok(()),
            }
        }
    }

    fn run_search(&mut self, out: &mut dyn Write) -> Result<(), AppError> {
        let query = self.query.clone();
        match search_uc::run(&mut self.app, &query, 1) {
            Ok(outcome) => {
                for w in &outcome.warnings {
                    self.renderer.warn(w);
                }
                self.session = search_uc::apply_to_session(&mut self.app, &outcome, &query)?;
                self.app.set_session(&self.session.session_id);
                self.documents.clear();
                writeln!(
                    out,
                    "search: {} results {} / {}",
                    outcome.hits.len(),
                    outcome
                        .filtered
                        .summary_line_for(outcome.hits.len(), outcome.truncated_by_limit),
                    self.quota_text(Metric::Searches)
                )
                .map_err(io_error)?;
                self.screen = Screen::Results;
            }
            Err(err) => {
                self.renderer.error(&err);
                if err.code == ErrorCode::E1007 {
                    self.screen = Screen::Query;
                    return Ok(());
                }
                self.screen = Screen::Query;
            }
        }
        Ok(())
    }

    fn show_results(&self, out: &mut dyn Write) -> Result<(), AppError> {
        let start = self
            .session
            .results
            .len()
            .saturating_sub(self.app.settings.ui.page_size);
        let _ = start;
        writeln!(
            out,
            "Q: {}    engine: {}",
            self.query, self.session.last_engine
        )
        .map_err(io_error)?;
        for hit in &self.session.results {
            let snippet = hit.snippet.clone().unwrap_or_default();
            let title_limit = self.app.settings.search.title_chars;
            let snippet_limit = self.app.settings.search.snippet_chars;
            writeln!(
                out,
                "[{}] {}",
                hit.label(),
                crate::core::text::truncate_display(&hit.title, title_limit)
            )
            .map_err(io_error)?;
            let secrets = crate::core::secret::redaction_list(&self.app.secrets);
            writeln!(out, "     {}", display_url(&hit.url, &secrets)).map_err(io_error)?;
            if !snippet.is_empty() {
                writeln!(
                    out,
                    "     {}",
                    crate::core::text::truncate_display(&snippet, snippet_limit)
                )
                .map_err(io_error)?;
            }
        }
        Ok(())
    }

    fn show_next(&mut self, out: &mut dyn Write) -> Result<(), AppError> {
        let query = self.query.clone();
        let page = self.session.page.clone();
        match search_uc::next(&mut self.app, &query, &page) {
            Ok(outcome) => {
                let session = search_uc::apply_to_session(&mut self.app, &outcome, &query)?;
                self.session = session;
                writeln!(
                    out,
                    "added {} results (page {}); searches {}/{}",
                    outcome.hits.len(),
                    outcome.page.page,
                    self.quota_used(Metric::Searches),
                    self.quota_limit(Metric::Searches)
                )
                .map_err(io_error)?;
                self.screen = Screen::Results;
            }
            Err(err) => {
                self.renderer.error(&err);
                self.screen = Screen::Results;
            }
        }
        Ok(())
    }

    fn fetch_selected(
        &mut self,
        out: &mut dyn Write,
        selections: &[select::Selection],
    ) -> Result<(), AppError> {
        let mut ids: Vec<u32> = Vec::new();
        for selection in selections {
            for r in &selection.refs {
                if let Ref::Id { prefix, id } = r
                    && *prefix == 'R'
                {
                    ids.push(*id);
                }
            }
        }
        if ids.is_empty() {
            writeln!(out, "select results (e.g. 1,3)").map_err(io_error)?;
            return Ok(());
        }
        self.documents.clear();
        for id in ids {
            let Some(hit) = self.session.find_result(id) else {
                writeln!(out, "R{id} is not in the list").map_err(io_error)?;
                continue;
            };
            let url = crate::core::url::parse(&hit.url)?;
            match document::fetch(&mut self.app, &url) {
                Ok(fetched) => {
                    for w in &fetched.warnings {
                        self.renderer.warn(w);
                    }
                    writeln!(
                        out,
                        "fetched D{} {} chars / {}",
                        fetched.document.id,
                        fetched.document.chars,
                        self.quota_text(Metric::Fetches)
                    )
                    .map_err(io_error)?;
                    self.reload_session()?;
                    self.documents.push(fetched.document);
                }
                Err(err) => self.renderer.error(&err),
            }
        }
        Ok(())
    }

    fn show_outline(&mut self, out: &mut dyn Write) -> Result<(), AppError> {
        let documents = self.documents.clone();
        for doc in &documents {
            match document::outline(&mut self.app, doc, self.depth) {
                Ok(view) => {
                    let secrets = crate::core::secret::redaction_list(&self.app.secrets);
                    let url = display_url(&view.document.final_url, &secrets);
                    let text = self.renderer.outline_lines(
                        &view.document,
                        &url,
                        &view.headings,
                        view.depth,
                    );
                    write!(out, "{text}").map_err(io_error)?;
                    if view.whole_body {
                        writeln!(
                            out,
                            "  (no headings found; the whole body is available as S0)"
                        )
                        .map_err(io_error)?;
                    }
                }
                Err(err) => self.renderer.error(&err),
            }
        }
        Ok(())
    }

    fn read_sections(
        &mut self,
        out: &mut dyn Write,
        selections: &[select::Selection],
    ) -> Result<(), AppError> {
        // 文書ごとに節をまとめる（同一文書内は出現順、文書間は指定順）
        let mut by_doc: Vec<(u32, Vec<u32>)> = Vec::new();
        let single = if self.documents.len() == 1 {
            self.documents.first().map(|d| d.id)
        } else {
            None
        };
        for selection in selections {
            let document_id = selection.document.or(single);
            let Some(document_id) = document_id else {
                writeln!(out, "specify a document (e.g. D1:S3)").map_err(io_error)?;
                continue;
            };
            let mut sections = Vec::new();
            for r in &selection.refs {
                match r {
                    Ref::Id { prefix: 'S', id } => sections.push(*id),
                    Ref::Id { prefix: 'D', id } => {
                        // `D1` 単独は全文
                        let _ = id;
                        sections.push(0);
                    }
                    Ref::Index(n) => sections.push(*n),
                    _ => {}
                }
            }
            match by_doc.iter_mut().find(|(d, _)| *d == document_id) {
                Some((_, list)) => list.extend(sections),
                None => by_doc.push((document_id, sections)),
            }
        }

        let mut chosen: Vec<(DocumentRef, Vec<u32>)> = Vec::new();
        for (document_id, sections) in by_doc {
            let Some(doc) = self.session.find_document(document_id).cloned() else {
                writeln!(out, "D{document_id} has not been fetched").map_err(io_error)?;
                continue;
            };
            chosen.push((doc, sections));
        }
        if chosen.is_empty() {
            return Ok(());
        }

        let include_children = self.app.settings.context.include_children;
        let mut record: Vec<(u32, u32)> = Vec::new();
        for (doc, sections) in chosen {
            match document::read(&mut self.app, &doc, &sections, include_children) {
                Ok(outcome) => {
                    for body in &outcome.sections {
                        let header = document::section_header(&doc, body);
                        let truncated = if body.truncated {
                            Some((crate::core::text::chars(&body.text), body.chars))
                        } else {
                            None
                        };
                        let secrets = crate::core::secret::redaction_list(&self.app.secrets);
                        let block = self.renderer.section_block(
                            &header,
                            &display_url(&doc.final_url, &secrets),
                            &body.text,
                            truncated,
                        );
                        write!(out, "{block}").map_err(io_error)?;
                        record.push((doc.id, body.heading_id));
                    }
                    writeln!(
                        out,
                        "(output {} chars / budget {} chars / {})",
                        outcome.total_chars,
                        outcome.budget,
                        self.quota_text(Metric::Chars)
                    )
                    .map_err(io_error)?;
                }
                Err(err) => self.renderer.error(&err),
            }
        }
        if !record.is_empty() {
            let _ = document::record_selection(&mut self.app, &record);
        }
        Ok(())
    }

    fn show_history(&mut self, out: &mut dyn Write) -> Result<(), AppError> {
        let session_id = self.app.current_session_id();
        let session_filter = (!session_id.is_empty()).then_some(session_id.as_str());
        match history_uc::list(&self.app, Some(20), session_filter) {
            Ok(entries) => {
                if entries.is_empty() {
                    writeln!(out, "no history entries").map_err(io_error)?;
                    return Ok(());
                }
                for entry in entries {
                    writeln!(out, "{}", self.renderer.history_line(&entry)).map_err(io_error)?;
                }
            }
            Err(err) => self.renderer.error(&err),
        }
        Ok(())
    }

    fn show_help(&mut self, out: &mut dyn Write) -> Result<(), AppError> {
        writeln!(
            out,
            "commands: q=quit  n=next page  r=edit query  s=history  h=help"
        )
        .map_err(io_error)?;
        Ok(())
    }

    /// 不正入力を数え、3 回で強制的に戻す。
    fn invalid(&mut self, out: &mut dyn Write) -> Result<bool, AppError> {
        self.invalid += 1;
        writeln!(
            out,
            "cannot parse the input (attempt {} of 3 before returning)",
            self.invalid
        )
        .map_err(io_error)?;
        if self.invalid >= 3 {
            self.invalid = 0;
            return Ok(true);
        }
        Ok(false)
    }

    fn reload_session(&mut self) -> Result<(), AppError> {
        let id = self.session.session_id.clone();
        if let Some(session) = self.app.sessions.load(&id)? {
            self.session = session;
        }
        Ok(())
    }

    fn quota_text(&self, metric: Metric) -> String {
        match self.app.guard() {
            Ok(guard) => guard.usage_line(metric),
            Err(_) => String::new(),
        }
    }

    /// セッション内の消費量（表示用）。
    fn quota_used(&self, metric: Metric) -> u64 {
        match self.app.guard() {
            Ok(guard) => guard.session_usage().get(metric),
            Err(_) => 0,
        }
    }

    /// セッション上限（`None` は無制限なので `0`）。
    fn quota_limit(&self, metric: Metric) -> u64 {
        match self.app.guard() {
            Ok(guard) => guard
                .metrics()
                .into_iter()
                .find(|m| m.name == format!("session.{}", metric.as_str()))
                .and_then(|m| m.limit)
                .unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// 画面（テスト用）。
    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// 現在の文書一覧（テスト用）。
    pub fn documents(&self) -> &[DocumentRef] {
        &self.documents
    }
}

fn read_line(input: &mut dyn BufRead) -> Result<Option<String>, AppError> {
    let mut line = String::new();
    match input.read_line(&mut line) {
        Ok(0) => Ok(None),
        Ok(_) => Ok(Some(line)),
        Err(e) => Err(AppError::with_detail(
            ErrorCode::E1001,
            format!("failed to read the input: {e}"),
        )),
    }
}

fn io_error(e: std::io::Error) -> AppError {
    AppError::with_detail(ErrorCode::E1001, format!("failed to write the output: {e}"))
}

/// 表示用 URL（秘匿パラメータを伏字化する）。
fn display_url(url: &str, secrets: &[String]) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => crate::core::url::redact(&parsed, secrets),
        Err(_) => crate::core::secret::redact_display_text(url, secrets),
    }
}
