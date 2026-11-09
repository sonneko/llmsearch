//! エンジン定義の読み込みと評価。

pub mod eval;
pub mod loader;

pub use eval::{EngineCall, evaluate};
pub use loader::{LoadedEngines, load_registry};
