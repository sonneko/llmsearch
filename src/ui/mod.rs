//! UI 層（引数解釈・描画・対話）。

pub mod cli;
pub mod commands;
pub mod interactive;
pub mod menu;
pub mod render;

use crate::core::errors::Diagnostic;
use crate::ports::Reporter;
use crate::ui::render::Renderer;

/// 標準エラーへ出力する `Reporter`。
pub struct StdReporter {
    renderer: std::sync::Arc<Renderer>,
    verbose: bool,
}

impl StdReporter {
    pub fn new(renderer: std::sync::Arc<Renderer>, verbose: bool) -> Self {
        Self { renderer, verbose }
    }
}

impl Reporter for StdReporter {
    fn info(&self, message: &str) {
        self.renderer.info(message);
    }

    fn warn(&self, diagnostic: &Diagnostic) {
        self.renderer.warn(diagnostic);
    }

    fn verbose(&self, message: &str) {
        if self.verbose {
            eprintln!("debug: {message}");
        }
    }
}
