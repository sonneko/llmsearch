//! 仕様の契約試験（JSON 定義の評価がエンジン非依存であること）。

use llmsearch::core::config::FilterSet;
use llmsearch::core::config::filter::{CompiledFilters, apply_content, apply_results};
use llmsearch::core::engine::{EngineRegistry, ResponseKind};
use llmsearch::core::extract::extract;
use llmsearch::core::model::SearchHit;

fn registry_of(json: serde_json::Value) -> EngineRegistry {
    let mut registry = EngineRegistry::new();
    let def = serde_json::from_value(json).expect("定義の解釈");
    registry.insert(def);
    registry
}

#[test]
fn same_definition_reads_html_and_json_without_engine_branches() {
    // HTML 版
    let html_def = registry_of(serde_json::json!({
        "schemaVersion": 1,
        "id": "sample",
        "request": {"url": "https://search.example/s", "query": {"q": "{{query}}"}},
        "response": {
            "kind": "html",
            "items": {"selector": "div.hit"},
            "fields": {
                "title": {"selector": "a", "transform": ["collapse_ws"]},
                "url": {"selector": "a", "attr": "href"}
            }
        }
    }));
    // JSON 版（同一 id・variant 違い）
    let json_def = registry_of(serde_json::json!({
        "schemaVersion": 1,
        "id": "sample",
        "variant": "api",
        "request": {"url": "https://api.example/s", "query": {"q": "{{query}}"}},
        "response": {
            "kind": "json",
            "items": {"path": "data.hits"},
            "fields": {
                "title": {"path": "name"},
                "url": {"path": "link"}
            }
        }
    }));

    let html = r#"<div class="hit"><a href="https://example.jp/a"> A </a></div>"#;
    let base = url::Url::parse("https://search.example/s").unwrap();
    let out = extract(&html_def.get("sample").unwrap().response, html, &base, &[]).unwrap();
    assert_eq!(out.hits[0].title, "A");

    let json = r#"{"data":{"hits":[{"name":" B ","link":"https://example.jp/b"}]}}"#;
    let out = extract(
        &json_def.get("sample@api").unwrap().response,
        json,
        &base,
        &[],
    )
    .unwrap();
    assert_eq!(out.hits[0].title, "B");
}

#[test]
fn unknown_transform_is_rejected() {
    let value = serde_json::json!({
        "schemaVersion": 1,
        "id": "bad",
        "request": {"url": "https://e/x"},
        "response": {
            "kind": "html",
            "items": {"selector": "li"},
            "fields": {
                "title": {"selector": "a", "transform": ["nope"]},
                "url": {"selector": "a"}
            }
        }
    });
    let err = serde_json::from_value::<llmsearch::core::engine::EngineDef>(value);
    assert!(err.is_err());
}

#[test]
fn error_when_marks_rate_limited_response_as_no_results() {
    let registry = registry_of(serde_json::json!({
        "schemaVersion": 1,
        "id": "e",
        "request": {"url": "https://e/x"},
        "response": {
            "kind": "json",
            "items": {"path": "results"},
            "fields": {"title": {"path": "t"}, "url": {"path": "u"}},
            "errorWhen": [{"contains": "rate limited"}]
        }
    }));
    let base = url::Url::parse("https://e/x").unwrap();
    let out = extract(
        &registry.get("e").unwrap().response,
        r#"{"message":"rate limited","results":[]}"#,
        &base,
        &[],
    )
    .unwrap();
    assert!(out.no_results);
}

#[test]
fn filters_can_be_replaced_by_json_only() {
    let set: FilterSet = serde_json::from_value(serde_json::json!({
        "schemaVersion": 2,
        "id": "custom",
        "results": {
            "require": [{"host": ["*.example.ac.jp"]}],
            "allow": [{"titleContains": ["例外"]}]
        }
    }))
    .unwrap();
    let compiled = CompiledFilters::compile(set).unwrap();
    let now = chrono::NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
    let hits = vec![
        seed(1, "研究", "https://lab.example.ac.jp/p"),
        seed(2, "ニュース", "https://news.example.jp/p"),
    ];
    let (kept, stats) = apply_results(&compiled, hits, now, false);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].title, "研究");
    assert_eq!(stats.removed, 1);
}

#[test]
fn content_filters_strip_boilerplate_lines() {
    let set: FilterSet = serde_json::from_value(serde_json::json!({
        "schemaVersion": 2,
        "id": "c",
        "content": {
            "stripLines": [{"boilerplate": ["Cookie"]}],
            "normalize": {"collapseBlankLines": true}
        }
    }))
    .unwrap();
    let compiled = CompiledFilters::compile(set).unwrap();
    let md = "# 見出し\n\n本文\n\nCookie ポリシーに同意する\n";
    let (out, stats) = apply_content(&compiled, md, false);
    assert!(!out.contains("Cookie"));
    assert!(out.contains("本文"));
    assert!(stats.removed >= 1);
}

#[test]
fn network_profile_switches_behaviour_from_json() {
    let strict: llmsearch::core::config::NetworkProfile =
        serde_json::from_value(serde_json::json!({
            "id": "strict",
            "timeoutTotalMs": 3000,
            "minIntervalMs": 5000,
            "maxConcurrency": 1,
            "robots": {"respect": true}
        }))
        .unwrap();
    assert_eq!(strict.timeout_total_ms, 3000);
    assert_eq!(strict.min_interval_ms, 5000);
    assert_eq!(strict.max_concurrency, 1);
    assert!(!strict.has_fail());
}

#[test]
fn paging_style_changes_request_parameters() {
    let value = serde_json::json!({
        "schemaVersion": 1,
        "id": "p",
        "request": {"url": "https://e/x", "query": {"q": "{{query}}", "start": "{{offset}}", "p": "{{page}}"}},
        "response": {
            "kind": "html",
            "items": {"selector": "li"},
            "fields": {"title": {"selector": "a"}, "url": {"selector": "a"}}
        },
        "paging": {"style": "page", "pageParam": "p", "pageBase": 1, "pageSize": 20, "maxResults": 100}
    });
    let def: llmsearch::core::engine::EngineDef = serde_json::from_value(value).unwrap();
    let (offset, page) = llmsearch::core::template::paging_values(def.paging.as_ref(), 2, 7);
    assert_eq!(offset, 0);
    assert_eq!(page, 2);
    assert_eq!(def.response.kind, ResponseKind::Html);
}

#[test]
fn hit_never_carries_body_text() {
    // `SearchHit` に本文フィールドが無いことを型で担保する（コンパイル時点で確認済み）。
    let hit = seed(1, "t", "https://e/1");
    let json = serde_json::to_value(&hit).unwrap();
    assert!(json.get("text").is_none());
    assert!(json.get("body").is_none());
    assert!(json.get("markdown").is_none());
}

fn seed(id: u32, title: &str, url: &str) -> SearchHit {
    SearchHit {
        id,
        rank: id,
        title: title.into(),
        url: url.into(),
        site: None,
        snippet: None,
        published: None,
        engine: "e".into(),
        language: None,
    }
}

/// 同梱の HTML エンジンは、結果 URL からリダイレクトを解く変換を必ず持つ。
///
/// これが抜けると「検索結果を開いたらエンジンのページだった」状態になる。
#[test]
fn bundled_html_engines_unwrap_redirects() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/engines");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("assets/engines") {
        let path = entry.expect("entry").path();
        if path.extension().map(|e| e != "json").unwrap_or(true) {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("読み込み");
        let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
        if value["response"]["kind"] != "html" {
            continue;
        }
        let transforms: Vec<String> = value["response"]["fields"]["url"]["transform"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            transforms.iter().any(|t| t == "unwrap_redirect"),
            "{} の url 変換に unwrap_redirect が無い: {transforms:?}",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 1, "HTML エンジンが 1 つも無い");
}
