//! `doctor`（自己診断）。

use crate::core::errors::{Check, ErrorCode};
use crate::usecases::App;

/// 診断結果 1 件。
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub check: Check,
}

/// 診断を実行する。
pub fn run(app: &App) -> Vec<CheckResult> {
    let mut out = Vec::new();
    out.push(CheckResult {
        name: "version".into(),
        check: Check::Ok(format!(
            "llmsearch {} / rustc {}",
            crate::VERSION,
            rustc_version()
        )),
    });

    let state = &app.paths.root;
    let state_ok = state.exists() && state.is_dir();
    out.push(CheckResult {
        name: "state.dir".into(),
        check: if state_ok {
            Check::Ok(state.display().to_string())
        } else {
            Check::Fail(format!("{} が無い", state.display()))
        },
    });

    // 書き込み確認
    let probe = state.join(".write-probe");
    let writable = match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    };
    out.push(CheckResult {
        name: "state.writable".into(),
        check: if writable {
            Check::Ok("書き込み可能".into())
        } else {
            Check::Fail(format!("{} に書き込めない", state.display()))
        },
    });

    out.push(CheckResult {
        name: "network.profile".into(),
        check: Check::Ok(app.http.describe(&app.network)),
    });
    for check in app.network.validate() {
        out.push(CheckResult {
            name: "network".into(),
            check,
        });
    }
    for warning in crate::adapters::http::profile_warnings(&app.network) {
        out.push(CheckResult {
            name: "network.policy".into(),
            check: Check::Warn(warning.message),
        });
    }

    out.push(CheckResult {
        name: "filters".into(),
        check: Check::Ok(format!(
            "{} 件の規則（deny {} / allow {} / require {}）",
            app.filters.deny.len() + app.filters.allow.len() + app.filters.require.len(),
            app.filters.deny.len(),
            app.filters.allow.len(),
            app.filters.require.len()
        )),
    });

    let quota_checks = crate::core::config::quota::validate(&app.quota);
    for check in quota_checks {
        out.push(CheckResult {
            name: "quota".into(),
            check,
        });
    }

    out.push(CheckResult {
        name: "engines".into(),
        check: Check::Ok(format!(
            "{} 件（有効 {} 件）",
            app.engines.len(),
            app.engines.enabled().count()
        )),
    });
    for (selector, checks) in crate::usecases::engines::validate(app, &[]) {
        for check in checks {
            out.push(CheckResult {
                name: format!("engine {selector}"),
                check,
            });
        }
    }

    if let Ok(guard) = app.guard() {
        let metrics = guard.metrics();
        if metrics.is_empty() {
            out.push(CheckResult {
                name: "quota.usage".into(),
                check: Check::Ok("上限は未設定".into()),
            });
        } else {
            for m in metrics {
                out.push(CheckResult {
                    name: format!("quota.{}", m.name),
                    check: Check::Ok(format!(
                        "{}/{}",
                        m.used,
                        m.limit
                            .map(|l| l.to_string())
                            .unwrap_or_else(|| "無制限".into())
                    )),
                });
            }
        }
    } else {
        out.push(CheckResult {
            name: "quota.usage".into(),
            check: Check::Fail("カウンタを読めない".into()),
        });
    }

    // ネットワーク到達性（1 回だけ・失敗しても継続）
    let probe_url = app
        .engines
        .get(&app.default_engine)
        .and_then(|d| crate::core::url::parse(&d.request.url).ok());
    match probe_url {
        Some(url) => {
            let request = crate::ports::http::HttpRequest::get(url.clone());
            match app.http.send_blocking(&app.network, &request) {
                Ok(response) => out.push(CheckResult {
                    name: "network.reachability".into(),
                    check: Check::Ok(format!(
                        "{} → HTTP {}",
                        url.host_str().unwrap_or("?"),
                        response.status
                    )),
                }),
                Err(err) => out.push(CheckResult {
                    name: "network.reachability".into(),
                    check: Check::Warn(format!("{}: {}", err.code.as_str(), err.message())),
                }),
            }
        }
        None => out.push(CheckResult {
            name: "network.reachability".into(),
            check: Check::Warn("確認先の URL を決められない".into()),
        }),
    }

    out.push(CheckResult {
        name: "history".into(),
        check: match app.history.query(&crate::ports::state::HistoryQuery {
            limit: Some(1),
            ..Default::default()
        }) {
            Ok(entries) => Check::Ok(format!("{} 件以上を読める", entries.len())),
            Err(err) => Check::Fail(format!("{}: {}", err.code.as_str(), err.message())),
        },
    });

    if app.settings.cache.enabled {
        out.push(CheckResult {
            name: "cache".into(),
            check: Check::Ok(format!("有効（TTL {} 時間）", app.settings.cache.ttl_hours)),
        });
    } else {
        out.push(CheckResult {
            name: "cache".into(),
            check: Check::Warn("無効".into()),
        });
    }

    let session = app.current_session_id();
    out.push(CheckResult {
        name: "session".into(),
        check: Check::Ok(format!(
            "{} / 既定エンジン {}",
            if session.is_empty() {
                "(未作成)"
            } else {
                session.as_str()
            },
            app.default_engine
        )),
    });

    if out.iter().any(|r| r.check.is_fail()) {
        out.push(CheckResult {
            name: "summary".into(),
            check: Check::Fail(format!(
                "{} 件の失敗（{}）",
                out.iter().filter(|r| r.check.is_fail()).count(),
                ErrorCode::E1004.as_str()
            )),
        });
    } else {
        out.push(CheckResult {
            name: "summary".into(),
            check: Check::Ok("重大な問題は無い".into()),
        });
    }

    out
}

fn rustc_version() -> &'static str {
    option_env!("RUSTC_VERSION").unwrap_or("(不明)")
}
