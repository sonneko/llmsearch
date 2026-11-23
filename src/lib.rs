//! llmsearch のライブラリ本体。
//!
//! 依存方向は `ui → usecases → core`、`usecases → ports ← adapters`。
//! `assets` は同梱既定の埋め込みだけを持つ（依存なし）。

pub mod adapters;
pub mod assets;
pub mod core;
pub mod ports;
pub mod ui;
pub mod usecases;

/// 版数。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
