//! llmsearch のライブラリ本体。
//!
//! 依存方向は `ui → usecases → core`、`usecases → ports ← adapters`。

pub mod adapters;
pub mod core;
pub mod ports;
pub mod ui;
pub mod usecases;

/// 版数。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
