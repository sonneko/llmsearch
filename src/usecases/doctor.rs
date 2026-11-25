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
pub fn run(app: &App, no_network: bool) -> Vec<CheckResult> {
    let mut out = Vec::new();
    out.push(CheckResult {
        name: "version".into(),
        check: Check::Ok(format!(
            "llmsearch {} / rustc {} / {}-{}",
            crate::VERSION,
            rustc_version(),
            std::env::consts::OS,
            std::env::consts::ARCH
        )),
    });

    let state = &app.paths.root;
    let state_ok = state.exists() && state.is_dir();
    out.push(CheckResult {
        name: "state.dir".into(),
        check: if state_ok {
            Check::Ok(state.display().to_string())
        } else {
            Check::Fail(format!("E5002: {} does not exist", state.display()))
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
            Check::Ok("writable".into())
        } else {
            Check::Fail(format!("E5002: cannot write to {}", state.display()))
        },
    });

    out.push(CheckResult {
        name: "network.profile".into(),
        check: Check::Ok(app.http.describe(&app.network)),
    });
    for check in app.network.validate() {
        out.push(CheckResult {
            name: "network".into(),
            check: with_code(check, ErrorCode::E1004),
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
            "{} rules (deny {} / allow {} / require {})",
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
            check: with_code(check, ErrorCode::E1004),
        });
    }

    out.push(CheckResult {
        name: "engines".into(),
        check: Check::Ok(format!(
            "{} entries ({} enabled)",
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
                check: Check::Ok("no limit configured".into()),
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
                            .unwrap_or_else(|| "unlimited".into())
                    )),
                });
            }
        }
    } else {
        out.push(CheckResult {
            name: "quota.usage".into(),
            check: Check::Fail("E5002: cannot read the counters".into()),
        });
    }

    // 到達性は `--no-network` で明示的に省略できる。
    if no_network {
        out.push(CheckResult {
            name: "network.reachability".into(),
            check: Check::Warn("skipped (--no-network)".into()),
        });
    } else {
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
                check: Check::Warn("cannot determine the URL to check".into()),
            }),
        }
    }

    out.push(CheckResult {
        name: "history".into(),
        check: match app.history.query(&crate::ports::state::HistoryQuery {
            limit: Some(1),
            ..Default::default()
        }) {
            Ok(entries) => Check::Ok(format!("read at least {} entries", entries.len())),
            Err(err) => Check::Fail(format!("{}: {}", err.code.as_str(), err.message())),
        },
    });

    if app.settings.cache.enabled {
        out.push(CheckResult {
            name: "cache".into(),
            check: Check::Ok(format!(
                "enabled (TTL {} hours)",
                app.settings.cache.ttl_hours
            )),
        });
    } else {
        out.push(CheckResult {
            name: "cache".into(),
            check: Check::Warn("disabled".into()),
        });
    }

    let session = app.current_session_id();
    out.push(CheckResult {
        name: "session".into(),
        check: Check::Ok(format!(
            "{} / default engine {}",
            if session.is_empty() {
                "(not created)"
            } else {
                session.as_str()
            },
            app.default_engine
        )),
    });

    let failure_count = out.iter().filter(|r| r.check.is_fail()).count();
    if failure_count > 0 {
        let codes = failure_codes(&out);
        out.push(CheckResult {
            name: "summary".into(),
            check: Check::Fail(format!("{failure_count} failures ({})", codes.join(", "))),
        });
    } else {
        out.push(CheckResult {
            name: "summary".into(),
            check: Check::Ok("no critical issues".into()),
        });
    }

    out
}

fn with_code(check: Check, code: ErrorCode) -> Check {
    match check {
        Check::Fail(text) if !text.contains(code.as_str()) => {
            Check::Fail(format!("{}: {text}", code.as_str()))
        }
        other => other,
    }
}

fn failure_codes(results: &[CheckResult]) -> Vec<String> {
    let mut codes = std::collections::BTreeSet::new();
    for result in results.iter().filter(|r| r.check.is_fail()) {
        let text = result.check.text();
        let bytes = text.as_bytes();
        let code = (0..bytes.len().saturating_sub(4)).find_map(|i| {
            (bytes[i] == b'E' && bytes[i + 1..i + 5].iter().all(u8::is_ascii_digit))
                .then(|| text[i..i + 5].to_string())
        });
        codes.insert(code.unwrap_or_else(|| {
            if result.name.starts_with("state")
                || result.name == "history"
                || result.name == "quota.usage"
            {
                ErrorCode::E5002.as_str().to_string()
            } else {
                ErrorCode::E1004.as_str().to_string()
            }
        }));
    }
    codes.into_iter().collect()
}

fn rustc_version() -> &'static str {
    option_env!("RUSTC_VERSION").unwrap_or("rustc version unavailable at build time")
}
