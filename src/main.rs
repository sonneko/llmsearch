//! llmsearch のエントリポイント。
//!
//! 依存の組み立てと、モード（対話・非対話）の選択のみを行う。

use std::io::BufReader;

use clap::Parser;

use llmsearch::core::errors::{AppError, ErrorCode};
use llmsearch::ui::cli::Cli;
use llmsearch::ui::interactive::Interactive;
use llmsearch::ui::render::Renderer;
use llmsearch::ui::{StdReporter, commands};
use llmsearch::usecases::config::{self as config_uc, Bootstrap};

fn main() {
    let cli = Cli::parse();
    let deps = config_uc::default_deps();
    let tty = deps.1.is_stdout_tty();
    let color = !cli.global.no_color && !cli.global.json && (cli.global.color || tty);
    let renderer = std::sync::Arc::new(Renderer::new(
        cli.global.json,
        color,
        cli.global.quiet,
        deps.1.terminal_width(),
        std::sync::Arc::clone(&deps.0),
    ));
    let reporter = std::sync::Arc::new(StdReporter::new(
        std::sync::Arc::clone(&renderer),
        cli.global.verbose,
    ));
    // JSON Lines に ANSI 装飾を混ぜず、CLI 要求との不一致を通知する。
    // `--quiet` は stderr の警告と、この独立した W1001 イベントを抑制する。
    if cli.global.color && cli.global.json && !cli.global.quiet {
        let warning = llmsearch::core::errors::Diagnostic::new(
            llmsearch::core::errors::WarningCode::W1001,
            "--color is ignored with --json to preserve machine-readable output",
        );
        renderer.warn_independent(&warning);
        renderer.warning_event(&[warning]);
    }

    let mut overrides = match config_uc::env_overrides(deps.1.as_ref()) {
        Ok(overrides) => overrides,
        Err(err) => {
            renderer.error(&err);
            std::process::exit(err.exit_code());
        }
    };
    if cli.global.no_cache {
        overrides.retain(|(k, _)| k != "cache.enabled");
        overrides.push(("cache.enabled".to_string(), serde_json::json!(false)));
    }
    if cli.global.no_filter {
        overrides.retain(|(k, _)| k != "filters");
    }
    let bootstrap = Bootstrap {
        config_path: cli.global.config.clone(),
        state_dir: cli.global.state_dir.clone(),
        engines_dirs: cli.global.engines_dir.clone(),
        network: cli.global.network.clone(),
        filters: cli.global.filters.clone(),
        quota: cli.global.quota.clone(),
        engine: cli.global.engine.clone(),
        language: cli.global.language.clone(),
        no_filter: cli.global.no_filter,
        no_cache: cli.global.no_cache,
        refresh: cli.global.refresh,
        json: cli.global.json,
        quiet: cli.global.quiet,
        verbose: cli.global.verbose,
        overrides,
    };

    let (mut app, resolved) = match config_uc::build(
        &bootstrap,
        std::sync::Arc::clone(&deps.0),
        std::sync::Arc::clone(&deps.1),
        std::sync::Arc::clone(&deps.2),
        reporter,
    ) {
        Ok(pair) => pair,
        Err(err) => {
            renderer.error(&err);
            std::process::exit(err.exit_code());
        }
    };

    renderer.set_secrets(app.redaction_list());

    if app.verbose {
        for check in &resolved.checks {
            if matches!(check, llmsearch::core::errors::Check::Ok(_)) {
                eprintln!("debug: [{}] {}", check.label(), check.text());
            }
        }
    }
    for warning in &resolved.warnings {
        renderer.warn(warning);
    }
    if let Some(id) = cli.global.session.as_deref() {
        renderer.set_session(id);
        if let Err(err) = app.select_session(id) {
            renderer.error(&err);
            std::process::exit(err.exit_code());
        }
    }

    let code = match &cli.command {
        Some(command) => match commands::run(&mut app, &renderer, &resolved, command) {
            Ok(code) => code,
            Err(err) => {
                renderer.set_session(app.current_session_id());
                renderer.error(&err);
                err.exit_code()
            }
        },
        None => run_interactive(app, renderer, cli.global.session.clone()),
    };
    std::process::exit(code);
}

fn run_interactive(
    app: llmsearch::usecases::App,
    renderer: std::sync::Arc<Renderer>,
    session: Option<String>,
) -> i32 {
    if !app.env.is_stdin_tty() {
        let err = AppError::with_detail(
            ErrorCode::E1001,
            "stdin is not a terminal (use a subcommand such as: llmsearch search \"...\")"
                .to_string(),
        );
        renderer.error(&err);
        return err.exit_code();
    }
    if let Some(id) = session.clone() {
        app.set_session(id);
    }
    match app.ensure_session() {
        Ok(id) => {
            app.set_session(&id);
            renderer.set_session(id);
        }
        Err(err) => {
            renderer.error(&err);
            return err.exit_code();
        }
    }
    let mut interactive = Interactive::new(app, (*renderer).clone_for_interactive());
    let stdin = std::io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let mut output = std::io::stdout();
    match interactive.run(&mut input, &mut output) {
        Ok(()) => 0,
        Err(err) => {
            renderer.error(&err);
            err.exit_code()
        }
    }
}
