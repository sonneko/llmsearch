//! コマンドライン引数の定義。

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "llmsearch",
    version,
    about = "Explore the web step by step like a human would, without burning the LLM context",
    long_about = None,
)]
pub struct Cli {
    /// Run without a subcommand to start interactive mode.
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub global: Global,
}

#[derive(Args, Debug, Clone, Default)]
pub struct Global {
    /// Emit JSON Lines on stdout.
    #[arg(long, global = true)]
    pub json: bool,

    /// Continue an existing session.
    #[arg(long, global = true, value_name = "ID")]
    pub session: Option<String>,

    /// Use another search engine for this run (`id` or `id@variant`).
    #[arg(long, global = true, value_name = "ID")]
    pub engine: Option<String>,

    /// Set the language sent to the search engine (`search.language`).
    #[arg(long, global = true, value_name = "TAG")]
    pub language: Option<String>,

    /// Use another network profile.
    #[arg(long, global = true, value_name = "ID")]
    pub network: Option<String>,

    /// Use another filter set.
    #[arg(long, global = true, value_name = "ID")]
    pub filters: Option<String>,

    /// Use another quota policy.
    #[arg(long, global = true, value_name = "ID")]
    pub quota: Option<String>,

    /// Disable the result and content filters.
    #[arg(long, global = true)]
    pub no_filter: bool,

    /// Do not read or write the cache.
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// Ignore the cache and fetch again.
    #[arg(long, global = true)]
    pub refresh: bool,

    /// Use an explicit configuration file.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Use an explicit state directory.
    #[arg(long, global = true, value_name = "PATH")]
    pub state_dir: Option<PathBuf>,

    /// Additional engine definition directories (repeatable).
    #[arg(long, global = true, value_name = "PATH")]
    pub engines_dir: Vec<PathBuf>,

    /// Disable decorations.
    #[arg(long, global = true, conflicts_with = "color")]
    pub no_color: bool,

    /// Decorate output even when stdout is not a terminal.
    #[arg(long, global = true)]
    pub color: bool,

    /// Suppress progress output.
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Add diagnostics to stderr.
    #[arg(long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Search the web and print the results.
    Search {
        query: String,
        /// Results per page (defaults to `search.limit`).
        #[arg(long)]
        limit: Option<usize>,
        /// Page number (1-based).
        #[arg(long, default_value_t = 1)]
        page: u32,
    },
    /// Fetch the next page of the same query.
    Next {
        #[arg(long)]
        query: Option<String>,
    },
    /// Change the query and search again.
    Refine { query: String },
    /// Fetch and convert a document, then print its outline.
    Outline {
        target: String,
        /// Outline depth, 2 to 4 (defaults to `context.outlineDepth`).
        #[arg(long)]
        depth: Option<u8>,
    },
    /// Print the text of a section.
    Read {
        target: String,
        /// Section (`D1:S3` or `S3`; `S0` is the whole document). Repeatable.
        #[arg(long = "section", value_name = "ID")]
        sections: Vec<String>,
        /// Include child sections (default). Use `--no-children` to exclude them.
        #[arg(long)]
        children: bool,
        /// Exclude child sections.
        #[arg(long = "no-children", conflicts_with = "children")]
        no_children: bool,
        /// Maximum characters to print.
        #[arg(long, value_name = "N")]
        max_chars: Option<usize>,
    },
    /// Browse, export, or clear the history.
    History {
        #[command(subcommand)]
        action: Option<HistoryAction>,
        /// Number of entries to show.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Keyword filter.
        #[arg(long)]
        query: Option<String>,
        /// Filter by event kind (repeatable).
        #[arg(long = "type")]
        kinds: Vec<String>,
        /// Only entries before this time.
        #[arg(long)]
        before: Option<String>,
        /// Delete without asking for confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Inspect, validate, or try engine definitions.
    Engines {
        #[command(subcommand)]
        action: Option<EnginesAction>,
    },
    /// Inspect or validate network profiles.
    Network {
        #[command(subcommand)]
        action: Option<NetworkAction>,
    },
    /// Inspect or validate filter sets.
    Filters {
        #[command(subcommand)]
        action: Option<FiltersAction>,
    },
    /// Inspect, reset, or check the quota.
    Quota {
        #[command(subcommand)]
        action: Option<QuotaAction>,
    },
    /// Show, initialize, or validate the configuration.
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Diagnose the environment, network, and configuration.
    Doctor,
}

#[derive(Subcommand, Debug, Clone)]
pub enum HistoryAction {
    /// List entries (default).
    List,
    /// Show one entry in detail.
    Show { id: String },
    /// Search entries by keyword.
    #[command(name = "grep")]
    Grep { query: String },
    /// Export as Markdown or JSON Lines.
    Export {
        #[arg(long, default_value = "markdown")]
        format: String,
    },
    /// Clear all entries.
    Clear {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum EnginesAction {
    /// List.
    List,
    /// Show a definition.
    Show {
        id: String,
        /// Print the raw JSON before resolution.
        #[arg(long)]
        raw: bool,
    },
    /// Validate.
    Validate { ids: Vec<String> },
    /// Run one live search.
    Test {
        id: String,
        #[arg(long)]
        query: String,
        /// Only print the assembled request; do not send it.
        #[arg(long)]
        dry_run: bool,
        /// Print the head of the raw response.
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
    /// Explain the resolved value and origin of one key.
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
            "climate change",
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
                assert_eq!(query, "climate change");
                assert_eq!(limit, Some(7));
                assert_eq!(page, 1);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn search_limit_defaults_to_none() {
        let cli = Cli::try_parse_from(["llmsearch", "search", "climate change"]).unwrap();
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
