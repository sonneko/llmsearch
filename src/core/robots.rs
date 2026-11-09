//! robots.txt の解釈（純粋ロジック）。

/// 判定結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny,
}

/// 1 つの `Allow` / `Disallow` 行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub allow: bool,
    pub pattern: String,
}

/// 1 つの User-agent 群。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Group {
    pub agents: Vec<String>,
    pub rules: Vec<Rule>,
    pub crawl_delay_ms: Option<u64>,
}

/// 解釈した robots.txt。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Robots {
    pub groups: Vec<Group>,
}

impl Robots {
    /// テキストを解釈する。解釈できない行は無視する。
    pub fn parse(text: &str, max_bytes: usize) -> Self {
        let mut robots = Robots::default();
        let mut current: Option<Group> = None;
        let mut last_was_agent = false;
        let mut seen = 0usize;

        for line in text.lines() {
            if seen >= max_bytes {
                break;
            }
            seen += line.len();
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            match key.as_str() {
                "user-agent" => {
                    if !last_was_agent {
                        if let Some(g) = current.take() {
                            robots.groups.push(g);
                        }
                        current = Some(Group::default());
                    }
                    if let Some(g) = current.as_mut() {
                        g.agents.push(value.to_ascii_lowercase());
                    }
                    last_was_agent = true;
                }
                "disallow" | "allow" => {
                    last_was_agent = false;
                    if let Some(g) = current.as_mut()
                        && !value.is_empty()
                    {
                        g.rules.push(Rule {
                            allow: key == "allow",
                            pattern: value,
                        });
                    }
                }
                "crawl-delay" => {
                    last_was_agent = false;
                    if let Some(g) = current.as_mut()
                        && let Ok(secs) = value.parse::<f64>()
                    {
                        g.crawl_delay_ms = Some((secs * 1000.0) as u64);
                    }
                }
                _ => {}
            }
        }
        if let Some(g) = current {
            robots.groups.push(g);
        }
        robots
    }

    /// 指定した User-Agent トークンにとっての判定。
    pub fn verdict(&self, token: &str, path: &str) -> Verdict {
        let token = token.to_ascii_lowercase();
        let matched: Vec<&Group> = self
            .groups
            .iter()
            .filter(|g| {
                g.agents
                    .iter()
                    .any(|a| a == "*" || (!a.is_empty() && token.contains(a.as_str())))
            })
            .collect();
        if matched.is_empty() {
            return Verdict::Allow;
        }
        // 最長一致の規則を採用する（同長なら Allow を優先）
        let mut best: Option<(usize, bool)> = None;
        for g in matched {
            for rule in &g.rules {
                if let Some(len) = pattern_match(&rule.pattern, path) {
                    let candidate = (len, rule.allow);
                    best = match best {
                        None => Some(candidate),
                        Some((blen, ballow)) => {
                            if len > blen || (len == blen && rule.allow && !ballow) {
                                Some(candidate)
                            } else {
                                Some((blen, ballow))
                            }
                        }
                    };
                }
            }
        }
        match best {
            Some((_, true)) => Verdict::Allow,
            Some((_, false)) => Verdict::Deny,
            None => Verdict::Allow,
        }
    }

    /// 一致した群の `crawl-delay`。
    pub fn crawl_delay_ms(&self, token: &str) -> Option<u64> {
        let token = token.to_ascii_lowercase();
        self.groups
            .iter()
            .filter(|g| {
                g.agents
                    .iter()
                    .any(|a| a == "*" || (!a.is_empty() && token.contains(a.as_str())))
            })
            .filter_map(|g| g.crawl_delay_ms)
            .max()
    }
}

/// パターン一致（`*` と末尾 `$` に対応）。一致した場合はパターン長を返す。
fn pattern_match(pattern: &str, path: &str) -> Option<usize> {
    if pattern.is_empty() {
        return None;
    }
    let anchored = pattern.ends_with('$');
    let body = pattern.trim_end_matches('$');
    let len = body.len();
    if !body.contains('*') {
        let hit = if anchored {
            path == body
        } else {
            path.starts_with(body)
        };
        return if hit { Some(len) } else { None };
    }
    // `*` を含む場合は簡易的な前方一致・中間一致
    let parts: Vec<&str> = body.split('*').collect();
    let mut cursor = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !path[cursor.min(path.len())..].starts_with(part) {
                return None;
            }
            cursor += part.len();
        } else if let Some(pos) = path[cursor.min(path.len())..].find(part) {
            cursor += pos + part.len();
        } else {
            return None;
        }
    }
    if anchored && !parts.last().map(|p| path.ends_with(p)).unwrap_or(false) {
        return None;
    }
    Some(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TXT: &str = "User-agent: *\nDisallow: /private\nAllow: /private/ok\n\n\
User-agent: badbot\nDisallow: /\n";

    #[test]
    fn allows_by_default() {
        let r = Robots::parse(TXT, 1024);
        assert_eq!(r.verdict("llmsearch", "/public"), Verdict::Allow);
        assert_eq!(r.verdict("llmsearch", "/private/x"), Verdict::Deny);
        assert_eq!(r.verdict("llmsearch", "/private/ok/1"), Verdict::Allow);
    }

    #[test]
    fn specific_agent_blocks_all() {
        let r = Robots::parse(TXT, 1024);
        assert_eq!(r.verdict("badbot/1.0", "/"), Verdict::Deny);
    }

    #[test]
    fn wildcard_and_anchor() {
        let r = Robots::parse("User-agent: *\nDisallow: /*.pdf$\n", 1024);
        assert_eq!(r.verdict("llmsearch", "/a/b.pdf"), Verdict::Deny);
        assert_eq!(r.verdict("llmsearch", "/a/b.pdf?x=1"), Verdict::Allow);
    }

    #[test]
    fn crawl_delay_is_parsed() {
        let r = Robots::parse("User-agent: *\nCrawl-delay: 1.5\n", 1024);
        assert_eq!(r.crawl_delay_ms("llmsearch"), Some(1500));
    }
}
