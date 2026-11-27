//! 秘匿値（API キー等）。表示時は伏字化する。

/// 秘匿値。`Debug` では中身を出さない。
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 実際の値。ログ・表示に使ってはならない。
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    pub fn masked(&self) -> String {
        if self.0.is_empty() {
            "(not set)".to_string()
        } else {
            "****".to_string()
        }
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secret(<redacted>)")
    }
}

/// 環境変数の解決結果を保持するコンテナ。
#[derive(Debug, Clone, Default)]
pub struct Secrets {
    values: std::collections::BTreeMap<String, Secret>,
}

impl Secrets {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(name.into(), Secret::new(value));
    }

    pub fn get(&self, name: &str) -> Option<&Secret> {
        self.values.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.values.keys()
    }

    pub fn value_of(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(|s| s.expose())
    }
}

/// 伏字化に使う文字列一覧（表示時の置換用）。
pub fn redaction_list(secrets: &Secrets) -> Vec<String> {
    secrets
        .names()
        .filter_map(|name| secrets.value_of(name))
        .filter(|value| value.chars().count() >= 4)
        .map(str::to_string)
        .collect()
}

/// 環境 secret に加え、エンジン定義の固定 `TmplValue::Explicit` および秘匿名の
/// ヘッダ・クエリ固定値も伏字化対象にする。
pub fn redaction_list_for_engines(
    secrets: &Secrets,
    engines: &crate::core::engine::EngineRegistry,
) -> Vec<String> {
    let mut values = redaction_list(secrets);
    for definition in engines.all() {
        collect_engine_secrets(definition, &mut values);
    }
    values
}

/// 環境 secret・エンジン定義・ネットワークプロファイルの固定秘匿値を集約する。
pub fn redaction_list_for_app(
    secrets: &Secrets,
    engines: &crate::core::engine::EngineRegistry,
    network: &crate::core::config::NetworkProfile,
) -> Vec<String> {
    let mut values = redaction_list_for_engines(secrets, engines);
    collect_network_secrets(network, &mut values);
    values
}

/// 単一のエンジン定義から固定秘匿値を抽出して追加する。
pub fn collect_engine_secrets(
    definition: &crate::core::engine::EngineDef,
    secrets: &mut Vec<String>,
) {
    if let Ok(value) = serde_json::to_value(definition) {
        collect_explicit_values(&value, secrets);
    }
}

/// ネットワークプロファイルから秘匿ヘッダ値・平文プロキシ認証値を抽出して追加する。
pub fn collect_network_secrets(
    network: &crate::core::config::NetworkProfile,
    secrets: &mut Vec<String>,
) {
    if let Ok(value) = serde_json::to_value(network) {
        collect_explicit_values(&value, secrets);
    }
}

fn push_secret_candidate(secrets: &mut Vec<String>, raw: &str) {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "***" || trimmed.contains("{{") {
        return;
    }
    if trimmed.chars().count() >= 4 && !secrets.iter().any(|s| s == trimmed) {
        secrets.push(trimmed.to_string());
    }
    if let Some((scheme, token)) = trimmed.split_once(char::is_whitespace) {
        let token = token.trim();
        if matches!(
            scheme.to_ascii_lowercase().as_str(),
            "bearer" | "basic" | "token" | "apikey" | "digest"
        ) && token != "***"
            && !token.contains("{{")
            && token.chars().count() >= 4
            && !secrets.iter().any(|s| s == token)
        {
            secrets.push(token.to_string());
        }
    }
}

/// 文字列に含まれる既知の秘匿値を `***` に置換する。
///
/// 短い値は誤爆を避けるため置換しない（`redaction_list` と同じ基準）。
pub fn redact_text(text: &str, secrets: &[String]) -> String {
    let mut ranges = Vec::new();
    for secret in secrets.iter().filter(|value| value.chars().count() >= 4) {
        let mut search_from = 0;
        loop {
            let Some(offset) = text
                .get(search_from..)
                .and_then(|remaining| remaining.find(secret.as_str()))
            else {
                break;
            };
            let start = search_from + offset;
            let end = start + secret.len();
            ranges.push((start, end));
            let Some(first_char) = text
                .get(start..)
                .and_then(|remaining| remaining.chars().next())
            else {
                break;
            };
            search_from = start + first_char.len_utf8();
        }
    }
    if ranges.is_empty() {
        return text.to_string();
    }

    ranges.sort_unstable();
    let mut merged = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some((_, last_end)) = merged.last_mut()
            && start <= *last_end
        {
            *last_end = (*last_end).max(end);
        } else {
            merged.push((start, end));
        }
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end) in merged {
        out.push_str(&text[cursor..start]);
        out.push_str("***");
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// 表示用の文字列を伏字化する。既知の値に加え、文中に埋め込まれた URL（任意スキームおよび
/// 構文エラー・未展開テンプレートを含む URL 様トークン）の userinfo と秘匿 query/path
/// パラメータも `core::url::redact_url_str` で処理する。
pub fn redact_display_text(text: &str, secrets: &[String]) -> String {
    let text = redact_text(text, secrets);
    let mut cursor = 0;
    let mut output = String::with_capacity(text.len());

    while let Some(rel_start) = find_scheme_url(&text[cursor..]) {
        let start = cursor + rel_start;
        output.push_str(&text[cursor..start]);
        let rest = &text[start..];
        let end = rest
            .char_indices()
            .find(|(_, ch)| {
                ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'' | '`' | '\\' | '|')
            })
            .map(|(index, _)| index)
            .unwrap_or(rest.len());
        let matched = &rest[..end];
        let candidate = trim_url_candidate(matched);
        let suffix = &matched[candidate.len()..];

        output.push_str(&crate::core::url::redact_url_str(candidate, secrets));
        output.push_str(suffix);
        cursor = start + end;
    }
    output.push_str(&text[cursor..]);
    output
}

fn find_scheme_url(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut search_from = 0;
    while let Some(rel_colon) = text.get(search_from..).and_then(|s| s.find("://")) {
        let colon_pos = search_from + rel_colon;
        let mut scheme_start = colon_pos;
        while scheme_start > search_from {
            let prev = bytes[scheme_start - 1];
            if prev.is_ascii_alphanumeric() || matches!(prev, b'+' | b'-' | b'.') {
                scheme_start -= 1;
            } else {
                break;
            }
        }
        if crate::core::url::is_valid_scheme(&text[scheme_start..colon_pos]) {
            return Some(scheme_start);
        }
        search_from = colon_pos + 3;
    }
    None
}

/// 文末の句読点や外側の閉じ括弧だけを URL トークン末尾から切り離す。
/// IPv6 リテラルの `]` やテンプレート変数 `{{...}}` の `}` は内側と釣り合っている限り残す。
fn trim_url_candidate(matched: &str) -> &str {
    let mut cur = matched;
    while let Some(ch) = cur.chars().next_back() {
        let strip = match ch {
            '.' | ',' | ';' | '!' | '?' => true,
            ':' => !cur.ends_with("://"),
            ')' => cur.matches('(').count() < cur.matches(')').count(),
            '}' => cur.matches('{').count() < cur.matches('}').count(),
            _ => false,
        };
        if strip {
            cur = &cur[..cur.len() - ch.len_utf8()];
        } else {
            break;
        }
    }
    cur
}

/// `Content-Type` や `Retry-After` など応答ヘッダ値に含まれる `; token=...` 等の
/// 秘匿パラメータと URL・既知秘匿値を伏せる（非秘匿の `charset=utf-8` 等は保持）。
pub fn redact_header_params(header_value: &str, secrets: &[String]) -> String {
    let text = redact_display_text(header_value, secrets);
    if !text.contains('=') {
        return text;
    }
    let mut changed = false;
    let parts: Vec<String> = text
        .split(';')
        .map(|part| {
            let trimmed_start = part.trim_start();
            let leading = &part[..part.len() - trimmed_start.len()];
            let trimmed = trimmed_start.trim_end();
            let trailing = &trimmed_start[trimmed.len()..];
            if let Some((name, val)) = trimmed.split_once('=') {
                let clean_name = name.trim();
                let clean_val = val.trim().trim_matches(['"', '\'']);
                if !clean_name.is_empty()
                    && !clean_val.is_empty()
                    && clean_val != "***"
                    && !clean_name.contains('/')
                    && !clean_name.contains(' ')
                    && crate::core::url::is_sensitive_param_name(clean_name)
                {
                    changed = true;
                    return format!("{leading}{clean_name}=***{trailing}");
                }
            }
            part.to_string()
        })
        .collect();
    if changed { parts.join(";") } else { text }
}

/// URL エンコードした form body、JSON body、HTTP ヘッダ行、および非構造テキスト中の
/// 埋込 JSON / key=value 秘匿フィールドを伏せる。非秘匿フィールドや `{{query}}` 等の
/// テンプレート記法は変質させずに保持する。
pub fn redact_body(body: &str, secrets: &[String]) -> String {
    let raw_trimmed = body.trim();
    if (raw_trimmed.starts_with('{') || raw_trimmed.starts_with('['))
        && let Ok(mut value) = serde_json::from_str::<serde_json::Value>(raw_trimmed)
    {
        let before = value.clone();
        redact_json_output(&mut value, secrets);
        return if value == before {
            redact_display_text(body, secrets)
        } else {
            serde_json::to_string(&value)
                .unwrap_or_else(|_| redact_unstructured_body(body, secrets))
        };
    }

    if is_form_urlencoded_body(raw_trimmed) {
        let leading_len = body.len() - body.trim_start().len();
        let trailing_start = leading_len + raw_trimmed.len();
        let redacted_form = redact_form_urlencoded(raw_trimmed, secrets);
        return format!(
            "{}{}{}",
            &body[..leading_len],
            redacted_form,
            &body[trailing_start..]
        );
    }

    redact_unstructured_body(body, secrets)
}

fn is_form_urlencoded_body(trimmed: &str) -> bool {
    !trimmed.is_empty()
        && !trimmed.contains('\n')
        && !trimmed.contains('\r')
        && !trimmed.contains('<')
        && trimmed.contains('=')
        && trimmed.split('&').all(|part| {
            let Some((name, _)) = part.split_once('=') else {
                return false;
            };
            !name.is_empty()
                && name
                    .bytes()
                    .all(|b| !b.is_ascii_whitespace() && !matches!(b, b':' | b'{' | b'}' | b'"'))
        })
}

fn redact_form_urlencoded(trimmed: &str, secrets: &[String]) -> String {
    trimmed
        .split('&')
        .map(|seg| {
            let Some((raw_name, raw_val)) = seg.split_once('=') else {
                return redact_display_text(seg, secrets);
            };
            let (decoded_name, decoded_val) = url::form_urlencoded::parse(seg.as_bytes())
                .next()
                .map(|(n, v)| (n.into_owned(), v.into_owned()))
                .unwrap_or_else(|| (raw_name.to_string(), raw_val.to_string()));
            let secret_value = secrets.iter().any(|secret| {
                secret.chars().count() >= 4
                    && (raw_val.contains(secret.as_str()) || decoded_val.contains(secret.as_str()))
            });
            let sensitive_name = crate::core::url::is_sensitive_param_name(&decoded_name);
            if (secret_value || sensitive_name) && raw_val != "***" {
                return format!("{raw_name}=***");
            }
            let red_raw = redact_display_text(raw_val, secrets);
            if red_raw != raw_val {
                return format!("{raw_name}={red_raw}");
            }
            let red_dec = redact_display_text(&decoded_val, secrets);
            if red_dec != decoded_val {
                let encoded: String =
                    url::form_urlencoded::byte_serialize(red_dec.as_bytes()).collect();
                return format!("{raw_name}={encoded}");
            }
            seg.to_string()
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn redact_unstructured_body(body: &str, secrets: &[String]) -> String {
    let step1 = redact_header_lines(body);
    let step2 = redact_inline_json_pairs(&step1);
    let step3 = redact_inline_kv_pairs(&step2);
    redact_display_text(&step3, secrets)
}

fn redact_header_lines(text: &str) -> String {
    if !text.contains(':') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let (content, ending) = if let Some(stripped) = line.strip_suffix("\r\n") {
            (stripped, "\r\n")
        } else if let Some(stripped) = line.strip_suffix('\n') {
            (stripped, "\n")
        } else {
            (line, "")
        };
        let trimmed_start = content.trim_start();
        let indent = &content[..content.len() - trimmed_start.len()];
        if let Some((name, val)) = trimmed_start.split_once(':')
            && !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            && is_sensitive_header_name(name)
            && !val.trim().is_empty()
            && val.trim() != "***"
        {
            out.push_str(indent);
            out.push_str(name);
            out.push_str(": ***");
            out.push_str(ending);
        } else {
            out.push_str(line);
        }
    }
    out
}

fn redact_inline_json_pairs(text: &str) -> String {
    if !text.contains('"') || !text.contains(':') {
        return text.to_string();
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let Some((key, after_key)) = parse_json_string_span(text, i) else {
            break;
        };
        let mut j = after_key;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b':' {
            i = after_key;
            continue;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'"' && is_sensitive_json_key(&key) {
            let val_content_start = j + 1;
            match parse_json_string_span(text, j) {
                Some((val_str, after_val)) => {
                    if !is_public_api_key_state_str(&key, &val_str) && val_str != "***" {
                        out.push_str(&text[cursor..val_content_start]);
                        out.push_str("***\"");
                        cursor = after_val;
                    }
                    i = after_val;
                    continue;
                }
                None => {
                    // Truncated mid-string at end of buffer: redact remaining partial secret!
                    out.push_str(&text[cursor..val_content_start]);
                    out.push_str("***");
                    cursor = bytes.len();
                    break;
                }
            }
        }
        i = after_key;
    }
    out.push_str(&text[cursor..]);
    out
}

fn parse_json_string_span(text: &str, open_quote: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    if bytes.get(open_quote) != Some(&b'"') {
        return None;
    }
    let mut escaped = false;
    let mut content = String::new();
    let mut idx = open_quote + 1;
    while idx < bytes.len() {
        let ch = text.get(idx..)?.chars().next()?;
        let next_idx = idx + ch.len_utf8();
        if escaped {
            content.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some((content, next_idx));
        } else {
            content.push(ch);
        }
        idx = next_idx;
    }
    None
}

fn redact_inline_kv_pairs(text: &str) -> String {
    if !text.contains('=') {
        return text.to_string();
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'=' {
            i += 1;
            continue;
        }
        let mut key_start = i;
        while key_start > cursor {
            let b = bytes[key_start - 1];
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
                key_start -= 1;
            } else {
                break;
            }
        }
        let left_ok = key_start == 0
            || matches!(
                bytes[key_start - 1],
                b'?' | b'&'
                    | b';'
                    | b' '
                    | b'\t'
                    | b'\r'
                    | b'\n'
                    | b'"'
                    | b'\''
                    | b'('
                    | b'['
                    | b'{'
                    | b','
                    | b'>'
            );
        let key = &text[key_start..i];
        if left_ok && !key.is_empty() && crate::core::url::is_sensitive_param_name(key) {
            let val_start = i + 1;
            // Skip URLs whose scheme:// appears before '='? Not possible since key is ASCII token.
            let mut val_end = val_start;
            while val_end < bytes.len() {
                let b = bytes[val_end];
                if b.is_ascii_whitespace()
                    || matches!(
                        b,
                        b'&' | b';' | b'"' | b'\'' | b')' | b']' | b'}' | b',' | b'<' | b'>'
                    )
                {
                    break;
                }
                val_end += 1;
            }
            // Ensure UTF-8 char boundary
            while val_end > val_start && !text.is_char_boundary(val_end) {
                val_end -= 1;
            }
            let val = &text[val_start..val_end];
            if !val.is_empty() && val != "***" {
                out.push_str(&text[cursor..val_start]);
                out.push_str("***");
                cursor = val_end;
                i = val_end;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&text[cursor..]);
    out
}

/// JSON 定義内の固定 `{"value": ...}` や秘匿名の headers/query の固定値を
/// redaction list に加える。短い値は誤爆防止のため対象外にし、既存値の重複も除く。
pub fn collect_explicit_values(value: &serde_json::Value, secrets: &mut Vec<String>) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                collect_explicit_values(item, secrets);
            }
        }
        serde_json::Value::Object(map) => {
            if !map.contains_key("value_env")
                && let Some(value) = map.get("value").and_then(serde_json::Value::as_str)
            {
                push_secret_candidate(secrets, value);
            }
            for (key, item) in map {
                if key.eq_ignore_ascii_case("headers")
                    && let serde_json::Value::Object(headers) = item
                {
                    for (hname, hval) in headers {
                        if is_sensitive_header_name(hname)
                            && let Some(s) = hval.as_str()
                        {
                            push_secret_candidate(secrets, s);
                        }
                    }
                } else if key.eq_ignore_ascii_case("query")
                    && let serde_json::Value::Object(query) = item
                {
                    for (qname, qval) in query {
                        if crate::core::url::is_sensitive_param_name(qname)
                            && let Some(s) = qval.as_str()
                        {
                            push_secret_candidate(secrets, s);
                        }
                    }
                }
                collect_explicit_values(item, secrets);
            }
        }
        _ => {}
    }
}

/// JSON 定義を表示する直前に伏字化する。
pub fn redact_json(value: &mut serde_json::Value, secrets: &Secrets) {
    let mut list = redaction_list(secrets);
    collect_explicit_values(value, &mut list);
    redact_json_with(value, &list);
}

/// JSON イベントなどの汎用出力を伏字化する。`value` キーを一律に伏せず、
/// 認証フィールド名・header 名・URL・body に限定して処理する。
pub fn redact_json_output(value: &mut serde_json::Value, secrets: &[String]) {
    redact_json_output_inner(value, secrets);
}

fn redact_json_output_inner(value: &mut serde_json::Value, secrets: &[String]) {
    match value {
        serde_json::Value::String(text) => *text = redact_display_text(text, secrets),
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_json_output_inner(item, secrets);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if key == "value_env" {
                    continue;
                }
                if key.eq_ignore_ascii_case("headers") {
                    redact_header_collection(item, secrets);
                } else if key.eq_ignore_ascii_case("body") {
                    if let serde_json::Value::String(body) = item {
                        *body = redact_body(body, secrets);
                    } else {
                        redact_json_output_inner(item, secrets);
                    }
                } else if is_sensitive_json_key(key) && !is_public_api_key_state(key, item) {
                    redact_sensitive_value(item);
                } else {
                    redact_json_output_inner(item, secrets);
                }
            }
        }
        _ => {}
    }
}

fn redact_header_collection(value: &mut serde_json::Value, secrets: &[String]) {
    match value {
        serde_json::Value::Object(headers) => {
            for (name, value) in headers.iter_mut() {
                if is_sensitive_header_name(name) {
                    redact_sensitive_value(value);
                } else {
                    redact_json_output_inner(value, secrets);
                }
            }
        }
        serde_json::Value::Array(rows) => {
            for row in rows.iter_mut() {
                match row {
                    serde_json::Value::Array(pair) if pair.len() >= 2 => {
                        let name = pair[0].as_str().unwrap_or_default();
                        if is_sensitive_header_name(name) {
                            redact_sensitive_value(&mut pair[1]);
                        } else {
                            redact_json_output_inner(&mut pair[1], secrets);
                        }
                    }
                    serde_json::Value::Object(headers) => {
                        for (name, value) in headers.iter_mut() {
                            if is_sensitive_header_name(name) {
                                redact_sensitive_value(value);
                            } else {
                                redact_json_output_inner(value, secrets);
                            }
                        }
                    }
                    _ => redact_json_output_inner(row, secrets),
                }
            }
        }
        _ => redact_json_output_inner(value, secrets),
    }
}

/// 文字列一覧を指定してエンジン等の JSON 定義を伏字化する。
/// `value_env` は環境変数名なので残す。`{"value": ...}` は固定値なので隠す。
pub fn redact_json_with(value: &mut serde_json::Value, secrets: &[String]) {
    redact_json_definition_inner(value, secrets);
}

fn redact_json_definition_inner(value: &mut serde_json::Value, secrets: &[String]) {
    match value {
        serde_json::Value::String(text) => *text = redact_display_text(text, secrets),
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_json_definition_inner(item, secrets);
            }
        }
        serde_json::Value::Object(map) => {
            if !map.contains_key("value_env")
                && let Some(serde_json::Value::String(text)) = map.get_mut("value")
            {
                *text = "***".to_string();
            }
            for (key, item) in map.iter_mut() {
                if key == "value_env" {
                    continue;
                }
                if key.eq_ignore_ascii_case("headers") {
                    redact_named_definition_entries(item, secrets, is_sensitive_header_name);
                } else if key.eq_ignore_ascii_case("query") {
                    redact_named_definition_entries(
                        item,
                        secrets,
                        crate::core::url::is_sensitive_param_name,
                    );
                } else if key.eq_ignore_ascii_case("body") {
                    if let serde_json::Value::String(body) = item {
                        *body = redact_body(body, secrets);
                    } else {
                        redact_json_definition_inner(item, secrets);
                    }
                } else if is_sensitive_json_key(key) {
                    redact_sensitive_value(item);
                } else {
                    redact_json_definition_inner(item, secrets);
                }
            }
        }
        _ => {}
    }
}

fn redact_named_definition_entries<F>(
    value: &mut serde_json::Value,
    secrets: &[String],
    is_sensitive: F,
) where
    F: Fn(&str) -> bool + Copy,
{
    match value {
        serde_json::Value::Object(entries) => {
            for (name, value) in entries.iter_mut() {
                if is_sensitive(name) {
                    redact_sensitive_value(value);
                } else {
                    redact_json_definition_inner(value, secrets);
                }
            }
        }
        _ => redact_json_definition_inner(value, secrets),
    }
}

fn redact_sensitive_value(value: &mut serde_json::Value) {
    if let serde_json::Value::Object(map) = value {
        if map.contains_key("value_env") {
            return;
        }
        if let Some(value) = map.get_mut("value") {
            *value = serde_json::json!("***");
            return;
        }
    }
    *value = serde_json::json!("***");
}

fn is_public_api_key_state(key: &str, value: &serde_json::Value) -> bool {
    value
        .as_str()
        .is_some_and(|state| is_public_api_key_state_str(key, state))
}

fn is_public_api_key_state_str(key: &str, state: &str) -> bool {
    key.eq_ignore_ascii_case("apiKey") && matches!(state, "not-required" | "configured" | "missing")
}

fn normalized_key(key: &str) -> String {
    key.to_ascii_lowercase().replace(['-', '_', ' ', '.'], "")
}

fn is_sensitive_json_key(key: &str) -> bool {
    matches!(
        normalized_key(key).as_str(),
        "authorization"
            | "proxyauthorization"
            | "wwwauthenticate"
            | "proxyauthenticate"
            | "cookie"
            | "setcookie"
            | "xapikey"
            | "apikey"
            | "apisecret"
            | "apitoken"
            | "xauthtoken"
            | "authtoken"
            | "sessiontoken"
            | "idtoken"
            | "bearertoken"
            | "csrftoken"
            | "token"
            | "password"
            | "passwd"
            | "secret"
            | "secretkey"
            | "privatekey"
            | "signature"
            | "sig"
            | "auth"
            | "accesstoken"
            | "refreshtoken"
            | "clientsecret"
            | "credential"
            | "credentials"
    )
}

/// ヘッダ名が秘匿対象かどうか（仕様 agent-integration.md §4.2）。
pub fn is_sensitive_header_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "authorization",
        "proxy-authorization",
        "cookie",
        "set-cookie",
        "api-key",
        "apikey",
        "auth",
        "token",
        "secret",
        "password",
        "passwd",
        "credential",
        "signature",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// 送信予定のヘッダから認証・ Cookie 類を伏せる（`--dry-run` の表示用）。
/// 値が環境変数から解決されたものでなくても、ヘッダ名だけで判定して伏せる。
pub fn redact_headers(headers: &mut [(String, String)]) {
    for (name, value) in headers.iter_mut() {
        if is_sensitive_header_name(name) {
            *value = "***".to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_leak() {
        let secret = Secret::new("topsecret");
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert_eq!(secret.expose(), "topsecret");
    }

    #[test]
    fn redaction_list_skips_short_values() {
        let mut secrets = Secrets::new();
        secrets.insert("A", "abcdef");
        secrets.insert("B", "xy");
        assert_eq!(redaction_list(&secrets), vec!["abcdef".to_string()]);
    }

    #[test]
    fn redact_text_hides_long_values_but_avoids_short_values() {
        let secrets = vec!["abcd1234".to_string(), "xy".to_string()];
        assert_eq!(
            redact_text("https://x.test/?k=abcd1234&x=xy", &secrets),
            "https://x.test/?k=***&x=xy"
        );
        assert_eq!(
            redact_text(
                "overlap=abcde; repeated=aaaaa",
                &["abcd".to_string(), "bcde".to_string(), "aaaa".to_string()]
            ),
            "overlap=***; repeated=***"
        );
    }

    #[test]
    fn redact_display_text_masks_embedded_url_credentials_and_parameters() {
        let text = "failed at https://alice:password@example.com/token=path-secret?p=public&token=query-secret.";
        let shown = redact_display_text(text, &[]);
        assert_eq!(
            shown,
            "failed at https://***:***@example.com/token=***?p=public&token=***."
        );
        assert!(!shown.contains("alice"));
        assert!(!shown.contains("password"));
        assert!(!shown.contains("path-secret"));
        assert!(!shown.contains("query-secret"));

        let ipv6 = redact_display_text("failed at http://alice:password@[::1]/path.", &[]);
        assert_eq!(ipv6, "failed at http://***:***@[::1]/path.");
    }

    #[test]
    fn redact_json_hides_explicit_values_and_keeps_env_names() {
        let mut value = serde_json::json!({
            "request": {
                "headers": {
                    "X-Api-Key": {"value": "plain-secret"},
                    "X-Session": {"value_env": "SESSION_TOKEN"}
                },
                "query": {"q": "{{query}}", "api_key": "literal-api-key"},
                "url": "https://x.test/path?token=plain-secret"
            }
        });
        redact_json_with(&mut value, &[]);
        assert_eq!(value["request"]["headers"]["X-Api-Key"]["value"], "***");
        assert_eq!(
            value["request"]["headers"]["X-Session"]["value_env"],
            "SESSION_TOKEN"
        );
        assert_eq!(value["request"]["query"]["q"], "{{query}}");
        assert_eq!(value["request"]["query"]["api_key"], "***");
        assert_eq!(value["request"]["url"], "https://x.test/path?token=***");
    }

    #[test]
    fn redact_json_output_masks_credentials_without_blanking_regular_values() {
        let mut value = serde_json::json!({
            "event": "diagnostics",
            "details": "request failed at https://user:pass@example.com/a?token=secret-value",
            "headers": [["X-Session", "visible"], ["Authorization", "Bearer literal-secret"]],
            "body": "{\"password\":\"literal-secret\",\"label\":\"visible\"}",
            "value": "ordinary value"
        });
        redact_json_output(&mut value, &[]);
        assert_eq!(
            value["details"],
            "request failed at https://***:***@example.com/a?token=***"
        );
        assert_eq!(value["headers"][0][1], "visible");
        assert_eq!(value["headers"][1][1], "***");
        let body: serde_json::Value =
            serde_json::from_str(value["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["password"], "***");
        assert_eq!(body["label"], "visible");
        assert_eq!(value["value"], "ordinary value");
    }

    #[test]
    fn redact_body_handles_form_encoded_secrets() {
        assert_eq!(
            redact_body("client_id=visible&access_token=secret123", &[]),
            "client_id=visible&access_token=***"
        );
        assert_eq!(redact_body("secret=***", &[]), "secret=***");
        assert_eq!(
            redact_body("q={{query}}&api_key=secret123&normal=KEEP", &[]),
            "q={{query}}&api_key=***&normal=KEEP"
        );
    }

    #[test]
    fn redact_body_and_headers_mask_raw_headers_truncated_json_and_params() {
        let raw_headers = "Authorization: Bearer RAW_BEARER_123\r\nSet-Cookie: sid=RAW_COOKIE_456\r\nX-Normal: KEEP_HDR\r\n";
        let red_headers = redact_body(raw_headers, &[]);
        assert_eq!(
            red_headers,
            "Authorization: ***\r\nSet-Cookie: ***\r\nX-Normal: KEEP_HDR\r\n"
        );

        let truncated_json = "{\"normal\":\"KEEP_JSON\",\"api_key\":\"TRUNC_SECRET_HALF";
        let red_trunc = redact_body(truncated_json, &[]);
        assert_eq!(red_trunc, "{\"normal\":\"KEEP_JSON\",\"api_key\":\"***");

        let ct = redact_header_params("application/pdf; token=CT_SECRET_789; charset=utf-8", &[]);
        assert_eq!(ct, "application/pdf; token=***; charset=utf-8");

        let non_http = redact_display_text(
            "cannot interpret ftp://u1:p1@127.0.0.1/doc as a target",
            &[],
        );
        assert_eq!(
            non_http,
            "cannot interpret ftp://***:***@127.0.0.1/doc as a target"
        );
    }

    #[test]
    fn collect_explicit_values_and_headers_use_one_masking_policy() {
        let value = serde_json::json!({
            "headers": {"X-Session": {"value": "static-secret"}},
            "description": "contains static-secret"
        });
        let mut list = Vec::new();
        collect_explicit_values(&value, &mut list);
        assert_eq!(list, ["static-secret"]);
        assert_eq!(
            redact_display_text("Bearer static-secret", &list),
            "Bearer ***"
        );

        let mut headers = vec![
            ("Accept".to_string(), "text/html".to_string()),
            ("X-Api-Key".to_string(), "literal-secret".to_string()),
            ("Authorization".to_string(), "Bearer tok".to_string()),
        ];
        redact_headers(&mut headers);
        assert_eq!(headers[0].1, "text/html");
        assert_eq!(headers[1].1, "***");
        assert_eq!(headers[2].1, "***");
    }
}
