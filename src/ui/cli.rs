//! コマンドライン引数の定義。

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "llmsearch",
    version,
    about = "Web を人間のように段階的に調べる CLI（LLM のコンテキストを節約する）",
    long_about = None,
)]
pub struct Cli {
    /// 引数なしで起動すると対話モードになる。
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub global: Global,
}

#[derive(Args, Debug, Clone, Default)]
pub struct Global {
    /// 標準出力を JSON Lines にする。
    #[arg(long, global = true)]
    pub json: bool,

    /// 既存セッションを継続する。
    #[arg(long, global = true, value_name = "ID")]
    pub session: Option<String>,

    /// 検索エンジンを一時的に切り替える（`id` または `id@variant`）。
    #[arg(long, global = true, value_name = "ID")]
    pub engine: Option<String>,

    /// ネットワークプロファイルを切り替える。
    #[arg(long, global = true, value_name = "ID")]
    pub network: Option<String>,

    /// フィルタ集合を切り替える。
    #[arg(long, global = true, value_name = "ID")]
    pub filters: Option<String>,

    /// クォータポリシーを切り替える。
    #[arg(long, global = true, value_name = "ID")]
    pub quota: Option<String>,

    /// 結果・本文フィルタを無効化する。
    #[arg(long, global = true)]
    pub no_filter: bool,

    /// キャッシュの読み書きを行わない。
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// キャッシュを無視して再取得する。
    #[arg(long, global = true)]
    pub refresh: bool,

    /// 設定ファイルを明示指定する。
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// 状態ディレクトリを明示指定する。
    #[arg(long, global = true, value_name = "PATH")]
    pub state_dir: Option<PathBuf>,

    /// エンジン定義の追加探索ディレクトリ（複数指定可）。
    #[arg(long, global = true, value_name = "PATH")]
    pub engines_dir: Vec<PathBuf>,

    /// 装飾を無効化する。
    #[arg(long, global = true, conflicts_with = "color")]
    pub no_color: bool,

    /// 端末でなくても装飾する。
    #[arg(long, global = true)]
    pub color: bool,

    /// 進捗表示を抑制する。
    #[arg(long, global = true)]
    pub quiet: bool,

    /// 診断情報を標準エラーへ追加する。
    #[arg(long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 検索して結果一覧を出力する。
    Search {
        query: String,
        /// 1 ページの件数（省略時は設定 `search.limit`）。
        #[arg(long)]
        limit: Option<usize>,
        /// ページ番号（1 始まり）。
        #[arg(long, default_value_t = 1)]
        page: u32,
    },
    /// 同一クエリの次ページを取得する。
    Next {
        #[arg(long)]
        query: Option<String>,
    },
    /// 検索語を変更して再検索する。
    Refine {
        query: String,
        #[arg(long)]
        previous: Option<String>,
    },
    /// 文書を取得・変換し、見出し一覧を出力する。
    Outline {
        target: String,
        /// 表示深度（2〜4。省略時は設定 `context.outlineDepth`）。
        #[arg(long)]
        depth: Option<u8>,
    },
    /// 節本文を出力する。
    Read {
        target: String,
        /// 節（`D1:S3` / `S3` / `0` で全文）。複数指定可。
        #[arg(long = "section", value_name = "ID")]
        sections: Vec<String>,
        /// 子見出しの本文を含める（既定）。`--no-children` で除外する。
        #[arg(long)]
        children: bool,
        /// 子見出しの本文を含めない。
        #[arg(long = "no-children", conflicts_with = "children")]
        no_children: bool,
        /// 出力の上限文字数。
        #[arg(long, value_name = "N")]
        max_chars: Option<usize>,
    },
    /// 閲覧履歴の参照・出力・削除。
    History {
        #[command(subcommand)]
        action: Option<HistoryAction>,
        /// 表示件数。
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// 絞り込みキーワード。
        #[arg(long)]
        query: Option<String>,
        /// 種別で絞り込む。
        #[arg(long = "type")]
        kinds: Vec<String>,
        /// この日時より前。
        #[arg(long)]
        before: Option<String>,
        /// 削除を確認なしで実行する。
        #[arg(long)]
        yes: bool,
    },
    /// エンジン定義の参照・検証・試行。
    Engines {
        #[command(subcommand)]
        action: Option<EnginesAction>,
    },
    /// ネットワークプロファイルの参照・検証。
    Network {
        #[command(subcommand)]
        action: Option<NetworkAction>,
    },
    /// フィルタ集合の参照・検証。
    Filters {
        #[command(subcommand)]
        action: Option<FiltersAction>,
    },
    /// クォータの参照・初期化・事前確認。
    Quota {
        #[command(subcommand)]
        action: Option<QuotaAction>,
    },
    /// 設定の表示・初期化・検証。
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// 環境・ネットワーク・設定の診断。
    Doctor,
}

#[derive(Subcommand, Debug, Clone)]
pub enum HistoryAction {
    /// 一覧（既定）。
    List,
    /// 1 件の詳細。
    Show { id: String },
    /// キーワード検索。
    #[command(name = "grep")]
    Grep { query: String },
    /// Markdown / JSON Lines で出力する。
    Export {
        #[arg(long, default_value = "markdown")]
        format: String,
    },
    /// 全削除。
    Clear {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum EnginesAction {
    /// 一覧。
    List,
    /// 定義の表示。
    Show {
        id: String,
        /// 解決前の JSON をそのまま表示する。
        #[arg(long)]
        raw: bool,
    },
    /// 検証。
    Validate { ids: Vec<String> },
    /// 1 回だけ試す。
    Test {
        id: String,
        #[arg(long)]
        query: String,
        /// リクエストの組み立て結果のみ表示する。
        #[arg(long)]
        dry_run: bool,
        /// 生の応答を先頭のみ表示する。
        #[arg(long)]
        raw: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum NetworkAction {
    List,
    Show { id: String },
    Validate { ids: Vec<String> },
}

#[derive(Subcommand, Debug, Clone)]
pub enum FiltersAction {
    List,
    Show {
        id: String,
        #[arg(long)]
        raw: bool,
    },
    Validate {
        ids: Vec<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum QuotaAction {
    Show,
    Reset {
        #[arg(long, default_value = "session")]
        scope: String,
        #[arg(long)]
        yes: bool,
    },
    Check {
        #[arg(long, default_value = "search")]
        kind: String,
        #[arg(long, default_value_t = 1)]
        amount: u64,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigAction {
    Show,
    Path,
    Init {
        #[arg(long)]
        force: bool,
    },
    Validate,
    /// 個別のキーについて、解決後の値と由来を説明する。
    Explain {
        key: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_with_globals() {
        let cli = Cli::try_parse_from([
            "llmsearch",
            "search",
            "気候変動",
            "--limit",
            "7",
            "--json",
            "--engine",
            "duckduckgo",
        ])
        .unwrap();
        assert!(cli.global.json);
        assert_eq!(cli.global.engine.as_deref(), Some("duckduckgo"));
        match cli.command {
            Some(Command::Search { query, limit, page }) => {
                assert_eq!(query, "気候変動");
                assert_eq!(limit, Some(7));
                assert_eq!(page, 1);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn search_limit_defaults_to_none() {
        let cli = Cli::try_parse_from(["llmsearch", "search", "気候変動"]).unwrap();
        match cli.command {
            Some(Command::Search { limit, .. }) => assert_eq!(limit, None),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn read_no_children_flag() {
        let cli = Cli::try_parse_from([
            "llmsearch",
            "read",
            "D1",
            "--section",
            "D1:S3",
            "--no-children",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Read {
                no_children,
                children,
                ..
            }) => {
                assert!(no_children);
                assert!(!children);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn no_subcommand_means_interactive() {
        let cli = Cli::try_parse_from(["llmsearch"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn read_accepts_multiple_sections() {
        let cli = Cli::try_parse_from([
            "llmsearch",
            "read",
            "D1",
            "--section",
            "D1:S3",
            "--section",
            "D1:S5",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Read { sections, .. }) => {
                assert_eq!(sections, vec!["D1:S3".to_string(), "D1:S5".to_string()]);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
