//! 対話メニューの状態遷移（純粋ロジック）と入力解釈。

use crate::core::select::{self, Selection};

/// 画面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// 検索語の入力。
    Query,
    /// 検索結果メニュー。
    Results,
    /// 結果の選択待ち。
    SelectResults,
    /// アウトライン（見出し）メニュー。
    Outline,
    /// 節の選択待ち。
    SelectSections,
    /// 履歴。
    History,
    /// 終了。
    Exit,
}

/// 検索結果メニューの選択肢。番号は仕様で固定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultsChoice {
    /// 1) 終了
    Quit,
    /// 2) 検索結果の選択（複数可）
    Select,
    /// 3) 8 件目以降を見る
    More,
    /// 4) 検索ワードを修正する
    Refine,
}

impl ResultsChoice {
    pub fn from_input(input: &str) -> Option<Self> {
        match input.trim() {
            "1" => Some(ResultsChoice::Quit),
            "2" => Some(ResultsChoice::Select),
            "3" => Some(ResultsChoice::More),
            "4" => Some(ResultsChoice::Refine),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ResultsChoice::Quit => "終了",
            ResultsChoice::Select => "検索結果の選択（複数可）",
            ResultsChoice::More => "8 件目以降を見る",
            ResultsChoice::Refine => "検索ワードを修正する",
        }
    }
}

/// アウトライン・メニューの選択肢。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineChoice {
    /// 1) 終了
    Quit,
    /// 2) 節を選んで読む
    Read,
    /// 3) 見出しの表示深度を変更する
    Depth,
    /// 4) 検索結果一覧に戻る
    Back,
    /// 5) 履歴を見る
    History,
}

impl OutlineChoice {
    pub fn from_input(input: &str) -> Option<Self> {
        match input.trim() {
            "1" => Some(OutlineChoice::Quit),
            "2" => Some(OutlineChoice::Read),
            "3" => Some(OutlineChoice::Depth),
            "4" => Some(OutlineChoice::Back),
            "5" => Some(OutlineChoice::History),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OutlineChoice::Quit => "終了",
            OutlineChoice::Read => "節を選んで読む（複数可）",
            OutlineChoice::Depth => "見出しの表示深度を変更する",
            OutlineChoice::Back => "検索結果一覧に戻る",
            OutlineChoice::History => "履歴を見る",
        }
    }
}

/// ユーザー入力の種別。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Choice(u8),
    Selection(Vec<Selection>),
    Query(String),
    Depth(u8),
    Empty,
    Interrupt,
    /// `h` / `?` ヘルプ、`b` 戻る等のコマンド。
    Command(String),
    Invalid(String),
}

/// 1 行の入力を解釈する。
pub fn parse_input(screen: Screen, raw: &str) -> Input {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Input::Empty;
    }
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "q" | "quit" | "exit" => return Input::Command("quit".into()),
        "h" | "?" | "help" => return Input::Command("help".into()),
        "b" | "back" => return Input::Command("back".into()),
        "n" | "next" => return Input::Command("next".into()),
        "r" | "refine" => return Input::Command("refine".into()),
        "s" | "history" => return Input::Command("history".into()),
        _ => {}
    }
    match screen {
        Screen::Results => match ResultsChoice::from_input(trimmed) {
            Some(c) => Input::Choice(match c {
                ResultsChoice::Quit => 1,
                ResultsChoice::Select => 2,
                ResultsChoice::More => 3,
                ResultsChoice::Refine => 4,
            }),
            None => Input::Invalid(trimmed.to_string()),
        },
        Screen::Outline => match OutlineChoice::from_input(trimmed) {
            Some(c) => Input::Choice(match c {
                OutlineChoice::Quit => 1,
                OutlineChoice::Read => 2,
                OutlineChoice::Depth => 3,
                OutlineChoice::Back => 4,
                OutlineChoice::History => 5,
            }),
            None => Input::Invalid(trimmed.to_string()),
        },
        Screen::SelectResults | Screen::SelectSections => match select::parse(trimmed) {
            Ok(sel) => Input::Selection(sel),
            Err(_) => Input::Invalid(trimmed.to_string()),
        },
        Screen::Query => {
            if trimmed.starts_with('!') {
                return Input::Command(format!("reuse:{trimmed}"));
            }
            Input::Query(trimmed.to_string())
        }
        _ => {
            if let Ok(n) = trimmed.parse::<u8>() {
                Input::Choice(n)
            } else {
                Input::Invalid(trimmed.to_string())
            }
        }
    }
}

/// 検索結果メニューの表示テキスト（番号は固定）。
pub fn results_menu() -> String {
    let mut out = String::from("  1) 終了\n  2) 検索結果の選択（複数可）\n");
    out.push_str("  3) 8 件目以降を見る\n  4) 検索ワードを修正する\n> ");
    out
}

/// アウトライン・メニューの表示テキスト。
pub fn outline_menu() -> String {
    let mut out = String::new();
    out.push_str("  1) 終了\n");
    out.push_str("  2) 節を選んで読む（複数可）\n");
    out.push_str("  3) 見出しの表示深度を変更する\n");
    out.push_str("  4) 検索結果一覧に戻る\n");
    out.push_str("  5) 履歴を見る\n> ");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_menu_numbers_are_fixed() {
        let menu = results_menu();
        assert!(menu.contains("1) 終了"));
        assert!(menu.contains("2) 検索結果の選択（複数可）"));
        assert!(menu.contains("3) 8 件目以降を見る"));
        assert!(menu.contains("4) 検索ワードを修正する"));
    }

    #[test]
    fn choices_map_to_actions() {
        assert_eq!(ResultsChoice::from_input("3"), Some(ResultsChoice::More));
        assert_eq!(ResultsChoice::from_input("4"), Some(ResultsChoice::Refine));
        assert_eq!(ResultsChoice::from_input("9"), None);
        assert_eq!(parse_input(Screen::Results, "1"), Input::Choice(1));
        assert_eq!(
            parse_input(Screen::Results, "q"),
            Input::Command("quit".into())
        );
    }

    #[test]
    fn selection_input_is_parsed() {
        match parse_input(Screen::SelectResults, "1,3-4") {
            Input::Selection(s) => assert_eq!(s[0].refs.len(), 3),
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(
            parse_input(Screen::SelectSections, "D1:S2"),
            Input::Selection(_)
        ));
        assert!(matches!(
            parse_input(Screen::SelectResults, "x"),
            Input::Invalid(_)
        ));
    }

    #[test]
    fn outline_choice_five_is_history() {
        assert_eq!(OutlineChoice::from_input("5"), Some(OutlineChoice::History));
        assert!(outline_menu().contains("5) 履歴を見る"));
    }
}
