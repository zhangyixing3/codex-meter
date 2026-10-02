use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::os::windows::process::CommandExt;
use std::{
    collections::BTreeMap,
    env, fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub remaining: f64,
    pub minutes: Option<i64>,
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResetCard {
    pub title: Option<String>,
    pub description: Option<String>,
    pub reset_type: Option<String>,
    pub status: Option<String>,
    pub granted_at: Option<i64>,
    pub expires_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResetCards {
    pub available_count: u64,
    pub cards: Option<Vec<ResetCard>>,
}

pub fn parse_reset_cards(v: &Value) -> Option<ResetCards> {
    Some(ResetCards {
        available_count: v["availableCount"].as_u64()?,
        cards: v["credits"].as_array().map(|rows| {
            rows.iter()
                .map(|r| ResetCard {
                    title: r["title"].as_str().map(str::to_owned),
                    description: r["description"].as_str().map(str::to_owned),
                    reset_type: r["resetType"].as_str().map(str::to_owned),
                    status: r["status"].as_str().map(str::to_owned),
                    granted_at: r["grantedAt"].as_i64(),
                    expires_at: r["expiresAt"].as_i64(),
                })
                .collect()
        }),
    })
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub primary: Option<Window>,
    pub secondary: Option<Window>,
    pub plan: Option<String>,
    pub quota_at: Option<i64>,
    pub usage_at: Option<i64>,
    pub lifetime: Option<u64>,
    pub days: Option<BTreeMap<NaiveDate, u64>>,
    pub reset_cards: Option<ResetCards>,
    pub error: Option<String>,
    pub usage_error: Option<String>,
    #[serde(skip)]
    pub refreshing: bool,
    #[serde(skip)]
    pub demo: bool,
    #[serde(skip)]
    pub notice: Option<String>,
}

pub fn request_refresh(s: &mut Snapshot, tx: &mpsc::Sender<()>) {
    if s.demo {
        s.notice = Some("演示模式 · 不联网刷新".into());
    } else if !s.refreshing {
        s.refreshing = true;
        s.notice = None;
        if tx.send(()).is_err() {
            s.refreshing = false;
            s.error = Some("刷新服务已停止，请退出后重新打开软件".into());
        }
    }
}

pub fn data_dir() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("CodexMeter")
}

impl Snapshot {
    pub fn load() -> Self {
        let mut s: Self = fs::read(data_dir().join("snapshot.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if s.quota_at.is_some() {
            s.error = Some("缓存数据，正在连接…".into());
        }
        s
    }
    pub fn save(&self) {
        let dir = data_dir();
        if fs::create_dir_all(&dir).is_ok() {
            if let Ok(bytes) = serde_json::to_vec(self) {
                // Replace only this application's sanitized aggregate cache.
                let _ = fs::write(dir.join("snapshot.json"), bytes);
            }
        }
    }
    pub fn today(&self) -> Option<u64> {
        // Missing buckets are unknown, never silently interpreted as zero.
        self.days.as_ref()?.get(&Local::now().date_naive()).copied()
    }
    pub fn demo() -> Self {
        let today = Local::now().date_naive();
        Self {
            primary: Some(Window {
                remaining: 74.0,
                minutes: Some(300),
                resets_at: Some(Local::now().timestamp() + 3600 * 3),
            }),
            secondary: Some(Window {
                remaining: 69.0,
                minutes: Some(10080),
                resets_at: Some(Local::now().timestamp() + 86400 * 4),
            }),
            lifetime: Some(4_300_000),
            reset_cards: Some(ResetCards {
                available_count: 3,
                cards: Some(
                    [3, 21, 28]
                        .into_iter()
                        .map(|days| ResetCard {
                            title: Some("完全重置（每周 + 5 小时）".into()),
                            description: None,
                            reset_type: Some("codexRateLimits".into()),
                            status: Some("available".into()),
                            granted_at: None,
                            expires_at: Some(Local::now().timestamp() + days * 86400),
                        })
                        .collect(),
                ),
            }),
            days: Some(
                (0..30)
                    .map(|i| {
                        (
                            today - chrono::Duration::days(i),
                            if i == 0 {
                                38200
                            } else {
                                (i as u64 * 17391) % 62000
                            },
                        )
                    })
                    .collect(),
            ),
            plan: Some("PRO".into()),
            quota_at: Some(Local::now().timestamp()),
            usage_at: Some(Local::now().timestamp()),
            demo: true,
            ..Self::default()
        }
    }
}

pub fn parse_window(v: &Value) -> Option<Window> {
    let used = v.get("usedPercent")?.as_f64()?;
    if !used.is_finite() {
        return None;
    }
    Some(Window {
        remaining: (100.0 - used).clamp(0.0, 100.0),
        minutes: v["windowDurationMins"].as_i64(),
        resets_at: v["resetsAt"].as_i64(),
    })
}

pub fn apply_quota(s: &mut Snapshot, v: &Value) -> Result<(), String> {
    s.reset_cards = parse_reset_cards(&v["rateLimitResetCredits"]);
    let buckets = v.get("rateLimitsByLimitId").and_then(Value::as_object);
    let bucket = if let Some(buckets) = buckets.filter(|b| !b.is_empty()) {
        buckets.get("codex").ok_or("账户没有返回 Codex 额度桶")?
    } else {
        &v["rateLimits"]
    };
    let primary = parse_window(&bucket["primary"]);
    let secondary = parse_window(&bucket["secondary"]);
    if primary.is_none() && secondary.is_none() {
        return Err("额度不可用（请用 ChatGPT 账户登录 Codex）".into());
    }
    s.primary = primary;
    s.secondary = secondary;
    s.plan = bucket["planType"].as_str().map(str::to_owned);
    s.quota_at = Some(Local::now().timestamp());
    s.error = None;
    Ok(())
}

pub fn apply_usage(s: &mut Snapshot, v: &Value) -> Result<(), String> {
    if !v["summary"].is_object() {
        return Err("Token 数据格式不受支持".into());
    }
    s.lifetime = v["summary"]["lifetimeTokens"].as_u64();
    s.days = v["dailyUsageBuckets"].as_array().map(|rows| {
        rows.iter()
            .filter_map(|r| {
                Some((
                    NaiveDate::parse_from_str(r["startDate"].as_str()?, "%Y-%m-%d").ok()?,
                    r["tokens"].as_u64()?,
                ))
            })
            .collect()
    });
    s.usage_at = Some(Local::now().timestamp());
    s.usage_error = None;
    Ok(())
}

pub fn find_codex() -> Option<PathBuf> {
    if let Some(p) = env::var_os("CODEX_METER_CODEX_PATH") {
        return PathBuf::from(p)
            .is_file()
            .then(|| PathBuf::from(env::var_os("CODEX_METER_CODEX_PATH").unwrap()));
    }
    let config = fs::read(data_dir().join("config.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    if let Some(p) = config.as_ref().and_then(|v| v["codex_path"].as_str()) {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(paths) = env::var_os("PATH") {
        for dir in env::split_paths(&paths) {
            let p = dir.join("codex.exe");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    if let Some(local) = env::var_os("LOCALAPPDATA") {
        let root = PathBuf::from(local).join("OpenAI/Codex/bin");
        let mut candidates: Vec<_> = fs::read_dir(root)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| e.path().join("codex.exe"))
            .filter(|p| p.is_file())
            .collect();
        candidates.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
        if let Some(p) = candidates.pop() {
            return Some(p);
        }
    }
    // npm installs wrap the executable with codex.cmd; launch the native binary directly.
    if let Some(appdata) = env::var_os("APPDATA") {
        let root = PathBuf::from(appdata).join("npm/node_modules/@openai");
        let mut pending = vec![(root, 0)];
        while let Some((dir, depth)) = pending.pop() {
            if depth > 8 {
                continue;
            }
            if let Ok(entries) = fs::read_dir(dir) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.file_name().is_some_and(|n| n == "codex.exe") {
                        return Some(p);
                    }
                    if p.is_dir() {
                        pending.push((p, depth + 1));
                    }
                }
            }
        }
    }
    None
}

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn send(child: &mut Child, value: Value) -> Result<(), String> {
    let pipe = child.stdin.as_mut().ok_or("Codex 通信已关闭")?;
    writeln!(pipe, "{value}")
        .and_then(|_| pipe.flush())
        .map_err(|_| "Codex 通信失败".into())
}

fn receive(rx: &mpsc::Receiver<Value>, id: u64, deadline: Instant) -> Result<Value, String> {
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        if wait.is_zero() {
            return Err("连接超时，稍后将自动重试".into());
        }
        let v = rx
            .recv_timeout(wait)
            .map_err(|_| "Codex 未响应（超时或进程退出）")?;
        if v["id"].as_u64() != Some(id) {
            continue;
        }
        if v.get("error").is_some() {
            return Err(match v["error"]["code"].as_i64() {
                Some(-32601) => "当前 Codex 版本不支持此接口，请更新 Codex",
                _ => "读取失败，请检查 Codex 登录状态和网络",
            }
            .into());
        }
        return v
            .get("result")
            .cloned()
            .ok_or_else(|| "Codex 返回了无效响应".into());
    }
}

pub fn refresh(s: &mut Snapshot) -> Result<(), String> {
    let path = find_codex().ok_or("未找到 Codex，请安装并登录 Codex 桌面版或 CLI")?;
    let mut server = Server(
        Command::new(path)
            .args(["app-server", "--listen", "stdio://"])
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "无法启动 Codex，请检查安装和 codex_path 设置")?,
    );
    let stdout = server.0.stdout.take().ok_or("无法读取 Codex 响应")?;
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        }
    });
    let result = (|| {
        let deadline = Instant::now() + Duration::from_secs(25);
        send(
            &mut server.0,
            json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"codex_meter","title":"Codex Meter","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}}),
        )?;
        receive(&rx, 1, deadline)?;
        send(&mut server.0, json!({"method":"initialized"}))?;
        send(
            &mut server.0,
            json!({"id":2,"method":"account/rateLimits/read"}),
        )?;
        send(&mut server.0, json!({"id":3,"method":"account/usage/read"}))?;
        // Replies may arrive out of order; consume both without discarding either.
        let mut quota = None;
        let mut usage = None;
        while quota.is_none() || usage.is_none() {
            let wait = deadline.saturating_duration_since(Instant::now());
            if wait.is_zero() {
                break;
            }
            let Ok(v) = rx.recv_timeout(wait) else {
                break;
            };
            let reply = if v.get("error").is_some() {
                Err("读取失败，请检查 Codex 登录、网络或版本".to_string())
            } else {
                v.get("result").cloned().ok_or("无效响应".to_string())
            };
            match v["id"].as_u64() {
                Some(2) => quota = Some(reply),
                Some(3) => usage = Some(reply),
                _ => {}
            }
        }
        s.usage_error = usage
            .unwrap_or_else(|| Err("Token 接口超时".into()))
            .and_then(|v| apply_usage(s, &v))
            .err();
        quota
            .unwrap_or_else(|| Err("额度接口超时".into()))
            .and_then(|v| apply_quota(s, &v))
    })();
    drop(server); // Always kill and reap, including errors and timeout paths.
    let _ = reader.join();
    result
}

pub fn compact(n: Option<u64>) -> String {
    match n {
        None => "—".into(),
        Some(n) if n >= 1_000_000_000 => format!("{:.2}B", n as f64 / 1e9),
        Some(n) if n >= 1_000_000 => format!("{:.2}M", n as f64 / 1e6),
        Some(n) if n >= 1000 => format!("{:.1}K", n as f64 / 1000.0),
        Some(n) => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reset_count_is_authoritative_and_details_are_optional() {
        let r = parse_reset_cards(
            &json!({"availableCount":3,"credits":[{"title":"Reset","expiresAt":123}]}),
        )
        .unwrap();
        assert_eq!(r.available_count, 3);
        assert_eq!(r.cards.unwrap()[0].expires_at, Some(123));
        assert!(parse_reset_cards(&Value::Null).is_none());
        assert!(
            parse_reset_cards(&json!({"availableCount":2,"credits":null}))
                .unwrap()
                .cards
                .is_none()
        );
        assert_eq!(
            parse_reset_cards(&json!({"availableCount":0,"credits":[]}))
                .unwrap()
                .available_count,
            0
        );
    }
    #[test]
    fn manual_refresh_acknowledges_and_debounces() {
        let (tx, rx) = mpsc::channel();
        let mut s = Snapshot::default();
        request_refresh(&mut s, &tx);
        assert!(s.refreshing);
        assert!(rx.try_recv().is_ok());
        request_refresh(&mut s, &tx);
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn disconnected_refresh_shows_failure() {
        let (tx, rx) = mpsc::channel();
        drop(rx);
        let mut s = Snapshot::default();
        request_refresh(&mut s, &tx);
        assert!(!s.refreshing);
        assert!(s.error.is_some());
    }
    #[test]
    fn remaining_and_null() {
        assert_eq!(
            parse_window(&json!({"usedPercent":26})).unwrap().remaining,
            74.0
        );
        assert!(parse_window(&Value::Null).is_none());
        assert_eq!(
            parse_window(&json!({"usedPercent":120})).unwrap().remaining,
            0.0
        );
    }
    #[test]
    fn prefer_codex_bucket() {
        let mut s = Snapshot::default();
        apply_quota(&mut s,&json!({"rateLimits":{"primary":{"usedPercent":90}},"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":20}},"other":{"primary":{"usedPercent":3}}}})).unwrap();
        assert_eq!(s.primary.unwrap().remaining, 80.0);
    }
    #[test]
    fn never_select_other_bucket() {
        let mut s = Snapshot::default();
        assert!(
            apply_quota(
                &mut s,
                &json!({"rateLimitsByLimitId":{"other":{"primary":{"usedPercent":3}}}})
            )
            .is_err()
        );
    }
    #[test]
    fn missing_is_not_zero() {
        let mut s = Snapshot::default();
        apply_usage(
            &mut s,
            &json!({"summary":{"lifetimeTokens":null},"dailyUsageBuckets":null}),
        )
        .unwrap();
        assert_eq!(s.today(), None);
        assert_eq!(s.lifetime, None);
    }
    #[test]
    fn dated_tokens() {
        let mut s = Snapshot::default();
        let today = Local::now().date_naive();
        apply_usage(&mut s,&json!({"summary":{"lifetimeTokens":38200},"dailyUsageBuckets":[{"startDate":today.to_string(),"tokens":38200}]})).unwrap();
        assert_eq!(s.today(), Some(38200));
        assert_eq!(s.lifetime, Some(38200));
    }
}
