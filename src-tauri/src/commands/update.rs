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
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_NOTES_CHARS: usize = 4096;

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
        .no_proxy()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3 || !allowed_api_url(attempt.url()) {
                attempt.error("更新检查仅允许 GitHub API HTTPS 重定向")
            } else {
                attempt.follow()
            }
        }))
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT);

    if let Some(address) = proxy {
        let address = if address.contains("://") {
            address.to_string()
        } else {
            format!("http://{address}")
        };
        builder = builder.proxy(reqwest::Proxy::all(address)?);
    }

    builder.build()
}

async fn fetch_latest(client: &reqwest::Client) -> Result<serde_json::Value, String> {
    let mut response = client
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

    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err("GitHub 更新响应超过 1 MiB 上限".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err("GitHub 更新响应超过 1 MiB 上限".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn allowed_api_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some("api.github.com")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
}

fn allowed_release_url(raw: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(raw) else {
        return false;
    };
    let root = format!("/{RELEASES_REPO}/releases");
    let tag_root = format!("{root}/tag/");
    url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && (url.path() == root
            || url
                .path()
                .strip_prefix(&tag_root)
                .is_some_and(|tag| !tag.is_empty() && !tag.contains('/')))
}

/// 把 `v1.2.3` / `1.2.3` 统一解析成 semver；解析失败返回 None。
fn parse_version(raw: &str) -> Option<semver::Version> {
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
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

    let (payload, used_proxy) = request_latest().await.map_err(|message| {
        AppError::Other(format!(
            "无法连接 GitHub 检查更新（{message}）。可以稍后再试，或直接访问 https://github.com/{RELEASES_REPO}/releases"
        ))
    })?;

    release_info(&current_raw, &payload, used_proxy)
}

fn release_info(
    current_raw: &str,
    payload: &serde_json::Value,
    used_proxy: bool,
) -> AppResult<UpdateInfo> {
    let current =
        parse_version(current_raw).ok_or_else(|| AppError::validation("当前应用版本格式无效"))?;

    let tag = payload
        .get("tag_name")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let release_url = payload
        .get("html_url")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    if !release_url.as_deref().is_some_and(allowed_release_url) {
        return Err(AppError::validation("GitHub 返回了无效的项目更新地址"));
    }
    let release_name = payload
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let published_at = payload
        .get("published_at")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let notes = payload
        .get("body")
        .and_then(|value| value.as_str())
        .map(|body| {
            body.lines()
                .take(16)
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .chars()
                .take(MAX_NOTES_CHARS)
                .collect::<String>()
                .to_string()
        });

    let latest = parse_version(&tag)
        .ok_or_else(|| AppError::validation("GitHub 发布版本格式无效，无法比较更新"))?;
    let has_update = latest > current;

    Ok(UpdateInfo {
        current_version: current_raw.to_string(),
        latest_version: Some(latest.to_string()),
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
        assert_eq!(
            pick_proxy_server("127.0.0.1:7890").as_deref(),
            Some("127.0.0.1:7890")
        );
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

    fn payload(tag: &str) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/{RELEASES_REPO}/releases/tag/{tag}"),
            "body": "中文更新说明"
        })
    }

    #[test]
    fn local_version_ahead_of_published_release_does_not_offer_a_downgrade() {
        assert!(
            !release_info("0.2.1", &payload("v0.2.0"), false)
                .unwrap()
                .has_update
        );
        assert!(
            !release_info("0.2.1", &payload("v0.2.1"), false)
                .unwrap()
                .has_update
        );
        assert!(
            release_info("0.2.1", &payload("v0.2.2"), false)
                .unwrap()
                .has_update
        );
    }

    #[test]
    fn invalid_versions_are_errors_instead_of_false_latest_status() {
        assert!(release_info("0.2.1", &payload("latest"), false).is_err());
        assert!(release_info("invalid", &payload("v0.2.2"), false).is_err());
    }

    #[test]
    fn release_links_are_scoped_to_this_projects_https_releases() {
        assert!(allowed_release_url(
            "https://github.com/kmoshan/shiftnote/releases"
        ));
        assert!(allowed_release_url(
            "https://github.com/kmoshan/shiftnote/releases/tag/v0.2.2"
        ));
        for url in [
            "http://github.com/kmoshan/shiftnote/releases",
            "https://example.com/releases",
            "file:///C:/notes.txt",
            "https://github.com/other/project/releases",
            "https://user@github.com/kmoshan/shiftnote/releases",
            "https://github.com/kmoshan/shiftnote/releases?redirect=elsewhere",
        ] {
            assert!(!allowed_release_url(url), "unexpected URL: {url}");
            let mut release = payload("v0.2.2");
            release["html_url"] = url.into();
            assert!(release_info("0.2.1", &release, false).is_err());
        }
    }

    #[test]
    fn release_notes_have_a_character_limit_as_well_as_line_limit() {
        let mut release = payload("v0.2.2");
        release["body"] = "中".repeat(5000).into();
        let info = release_info("0.2.1", &release, true).unwrap();
        assert_eq!(info.notes.unwrap().chars().count(), MAX_NOTES_CHARS);
        assert!(info.used_proxy);
    }

    #[test]
    fn redirect_targets_require_the_github_api_host_and_https() {
        for url in [
            "http://api.github.com/repos/kmoshan/shiftnote",
            "https://example.com/",
            "https://user@api.github.com/",
        ] {
            assert!(!allowed_api_url(&reqwest::Url::parse(url).unwrap()));
        }
        assert!(allowed_api_url(
            &reqwest::Url::parse(&latest_release_url()).unwrap()
        ));
    }

    #[test]
    #[ignore = "手工联网验证公开 GitHub Releases，不属于离线单测"]
    fn release_endpoint_smoke() {
        let (payload, used_proxy) = tauri::async_runtime::block_on(request_latest()).unwrap();
        let info = release_info(env!("CARGO_PKG_VERSION"), &payload, used_proxy).unwrap();
        println!(
            "current={}, latest={}, has_update={}, used_proxy={}",
            info.current_version,
            info.latest_version.unwrap(),
            info.has_update,
            info.used_proxy
        );
    }
}
