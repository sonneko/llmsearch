//! エンジン定義の読み込みと評価。

pub mod eval;
pub mod loader;

pub use eval::{EngineCall, evaluate, evaluate_with_preflight};
pub use loader::{LoadedEngines, load_registry};
