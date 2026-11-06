//! 检查更新。
//!
//! 只做“检测 + 提示”：读取 GitHub 仓库的最新 Release 与当前版本比较，
//! 有更新时由界面提示并引导用户打开下载页，不自动下载、不静默安装。
//!
//! 网络请求全部在 Rust 侧完成（WebView 不直接访问外网，CSP 保持收紧）。
//! 直连失败时会尝试读取 Windows 系统代理后重试，适配需要代理的网络环境。

use std::time::Duration;

use serde::Serialize;
use tauri::State;

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const RELEASES_REPO: &str = "kmoshan/shiftnote";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub has_update: bool,
    pub release_name: Option<String>,
    pub release_url: Option<String>,
    pub published_at: Option<String>,
    /// Release 说明的前若干行，供界面展示
    pub notes: Option<String>,
    pub checked_at: i64,
    /// 本次请求是否经由系统代理完成
    pub used_proxy: bool,
}

fn latest_release_url() -> String {
    format!("https://api.github.com/repos/{RELEASES_REPO}/releases/latest")
}

fn build_client(proxy: Option<&str>) -> reqwest::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .user_agent(concat!("ShiftNote/", env!("CARGO_PKG_VERSION")))
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT);

    if let Some(address) = proxy {
        builder = builder.proxy(reqwest::Proxy::all(format!("http://{address}"))?);
    }

    builder.build()
}

async fn fetch_latest(client: &reqwest::Client) -> Result<serde_json::Value, String> {
    let response = client
        .get(latest_release_url())
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| error.to_string())?;

    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err("仓库还没有发布任何 Release".into());
    }
    if !status.is_success() {
        return Err(format!("GitHub 返回 HTTP {}", status.as_u16()));
    }

    response
        .json::<serde_json::Value>()
        .await
        .map_err(|error| error.to_string())
}

/// 把 `v1.2.3` / `1.2.3` 统一解析成 semver；解析失败返回 None。
fn parse_version(raw: &str) -> Option<semver::Version> {
    let trimmed = raw.trim();
    let stripped = trimmed.strip_prefix('v').or_else(|| trimmed.strip_prefix('V')).unwrap_or(trimmed);
    semver::Version::parse(stripped).ok()
}

/// 解析系统代理地址，兼容 `host:port` 与 `http=h:p;https=h:p` 两种写法。
fn pick_proxy_server(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    if !value.contains('=') {
        return Some(value.to_string());
    }

    let mut fallback = None;
    for part in value.split(';') {
        let Some((scheme, address)) = part.split_once('=') else {
            continue;
        };
        let address = address.trim();
        if address.is_empty() {
            continue;
        }
        match scheme.trim().to_ascii_lowercase().as_str() {
            "https" => return Some(address.to_string()),
            "http" => fallback = Some(address.to_string()),
            _ => {}
        }
    }
    fallback
}

#[cfg(windows)]
fn system_proxy() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;

    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    if enabled == 0 {
        return None;
    }

    let server: String = key.get_value("ProxyServer").ok()?;
    pick_proxy_server(&server)
}

#[cfg(not(windows))]
fn system_proxy() -> Option<String> {
    std::env::var("HTTPS_PROXY")
        .or_else(|_| std::env::var("https_proxy"))
        .ok()
}

async fn request_latest() -> Result<(serde_json::Value, bool), String> {
    // 先直连
    match build_client(None) {
        Ok(client) => match fetch_latest(&client).await {
            Ok(value) => return Ok((value, false)),
            Err(direct_error) => {
                // 直连失败再尝试系统代理（只重试一次）
                if let Some(proxy) = system_proxy() {
                    if let Ok(client) = build_client(Some(&proxy)) {
                        if let Ok(value) = fetch_latest(&client).await {
                            return Ok((value, true));
                        }
                    }
                }
                return Err(direct_error);
            }
        },
        Err(error) => return Err(error.to_string()),
    }
}

#[tauri::command]
pub async fn check_update(state: State<'_, AppState>) -> AppResult<UpdateInfo> {
    let current_raw = state.info.version.clone();
    let current = parse_version(&current_raw);

    let (payload, used_proxy) = request_latest().await.map_err(|message| {
        AppError::Other(format!(
            "无法连接 GitHub 检查更新（{message}）。可以稍后再试，或直接访问 https://github.com/{RELEASES_REPO}/releases"
        ))
    })?;

    let tag = payload
        .get("tag_name")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let release_url = payload
        .get("html_url")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let release_name = payload
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let published_at = payload
        .get("published_at")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let notes = payload.get("body").and_then(|value| value.as_str()).map(|body| {
        body.lines()
            .take(16)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    });

    let latest = parse_version(&tag);
    let has_update = match (&current, &latest) {
        (Some(current), Some(latest)) => latest > current,
        _ => false,
    };

    Ok(UpdateInfo {
        current_version: current_raw,
        latest_version: latest
            .map(|version| version.to_string())
            .or_else(|| (!tag.is_empty()).then_some(tag)),
        has_update,
        release_name,
        release_url,
        published_at,
        notes,
        checked_at: now_ms(),
        used_proxy,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions_with_and_without_prefix() {
        assert_eq!(parse_version("v0.1.0").unwrap().to_string(), "0.1.0");
        assert_eq!(parse_version(" V1.2.3 ").unwrap().to_string(), "1.2.3");
        assert!(parse_version("latest").is_none());
    }

    #[test]
    fn compares_versions() {
        let current = parse_version("0.1.0").unwrap();
        assert!(parse_version("0.1.1").unwrap() > current);
        assert!(parse_version("0.2.0").unwrap() > current);
        assert!(parse_version("0.1.0").unwrap() <= current);
    }

    #[test]
    fn parses_proxy_forms() {
        assert_eq!(pick_proxy_server("127.0.0.1:7890").as_deref(), Some("127.0.0.1:7890"));
        assert_eq!(
            pick_proxy_server("http=127.0.0.1:7890;https=127.0.0.1:7891").as_deref(),
            Some("127.0.0.1:7891")
        );
        assert_eq!(
            pick_proxy_server("http=127.0.0.1:7890;ftp=127.0.0.1:2121").as_deref(),
            Some("127.0.0.1:7890")
        );
        assert_eq!(pick_proxy_server("   "), None);
    }
}
