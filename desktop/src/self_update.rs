//! 程序自更新（检查 GitHub Release 并替换当前可执行文件）。
//!
//! 参考 Tomato-Novel-Downloader 的 self_update 设计移植：
//! - 通过 GitHub Releases API 获取最新版本
//! - 选择匹配当前平台/架构的资产
//! - 可选使用 `https://dl.zhongbai233.com/` 加速（可通过 `RER_DISABLE_ACCEL=1` 禁用）
//! - 下载后按需校验 SHA256（若 Release 资产提供 digest）
//! - Windows 使用临时 .bat 进行替换并重启；Unix 直接替换并重启

use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const OWNER: &str = "zhongbai2333";
const REPO: &str = "RustEpubReader";

/// 当前编译版本号（来自 Cargo.toml）
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// 已是最新版本
    UpToDate,
    /// 跳过更新（Docker / 开发态 / 用户取消等）
    Skipped,
    /// 已启动更新流程（即将重启）
    UpdateLaunched,
}

/// 更新进度回调，签名: (已下载字节, 总字节)
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send>;

// ─── GitHub Release 数据结构 ───────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ReleaseInfo {
    name: Option<String>,
    tag_name: Option<String>,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    size: Option<u64>,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Debug, Clone)]
struct MatchedAsset {
    release_name: String,
    tag_name: String,
    download_url: String,
    size: u64,
    sha256: Option<String>,
}

// ─── 公开 API ──────────────────────────────────────────────────────

/// 检查最新版本信息（不下载）。
/// 返回 `Some((tag_name, release_name))` 如果有更新，`None` 如果已是最新或检查失败。
pub fn check_latest_version() -> Option<(String, String)> {
    let current_tag = format!("v{CURRENT_VERSION}");
    let matched = get_latest_release_asset().ok()?;

    if matched.tag_name != current_tag {
        Some((matched.tag_name, matched.release_name))
    } else {
        None
    }
}

/// 执行完整的更新检查与下载流程（阻塞）。
/// `on_progress` 可为 `None`，否则在下载期间回调进度。
pub fn perform_update(on_progress: Option<ProgressCallback>) -> Result<UpdateOutcome> {
    if is_dev_build() {
        return Ok(UpdateOutcome::Skipped);
    }

    let current_tag = format!("v{CURRENT_VERSION}");
    let matched = get_latest_release_asset()?;

    let is_new = matched.tag_name != current_tag;

    if !is_new {
        // 版本号相同 → 检查热补丁
        if let Some(expected) = matched.sha256.as_deref() {
            let self_hash = compute_file_sha256(&current_executable_path()?)?;
            if !eq_hash(&self_hash, expected) {
                start_update(&matched, on_progress)?;
                return Ok(UpdateOutcome::UpdateLaunched);
            }
        }
        return Ok(UpdateOutcome::UpToDate);
    }

    // 有新版本 → 下载并应用
    start_update(&matched, on_progress)?;
    Ok(UpdateOutcome::UpdateLaunched)
}

/// 仅检查热补丁（版本号相同但二进制不同）。
#[allow(dead_code)]
pub fn check_hotfix_and_apply() -> Result<UpdateOutcome> {
    if is_dev_build() {
        return Ok(UpdateOutcome::UpToDate);
    }

    let current_tag = format!("v{CURRENT_VERSION}");
    let matched = get_latest_release_asset()?;

    if matched.tag_name != current_tag {
        return Ok(UpdateOutcome::UpToDate);
    }

    if let Some(expected) = matched.sha256.as_deref() {
        let self_hash = compute_file_sha256(&current_executable_path()?)?;
        if !eq_hash(&self_hash, expected) {
            start_update(&matched, None)?;
            return Ok(UpdateOutcome::UpdateLaunched);
        }
    }

    Ok(UpdateOutcome::UpToDate)
}

// ─── 内部实现 ──────────────────────────────────────────────────────

fn eq_hash(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn is_dev_build() -> bool {
    if std::env::var_os("CARGO").is_some() {
        return true;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let s = exe.to_string_lossy().to_ascii_lowercase();
    s.contains("\\target\\debug\\")
        || s.contains("/target/debug/")
        || s.contains("\\target\\release\\")
        || s.contains("/target/release/")
}

fn build_http_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .context("init http client")
}

fn github_latest_release_url() -> String {
    format!("https://api.github.com/repos/{OWNER}/{REPO}/releases/latest")
}

fn fetch_latest_release(client: &Client) -> Result<ReleaseInfo> {
    let url = github_latest_release_url();
    let resp = client
        .get(url)
        .header(ACCEPT, "application/vnd.github+json")
        .header(USER_AGENT, "RustEpubReader/1.0")
        .send()
        .context("request latest release")?
        .error_for_status()
        .context("latest release status")?;

    resp.json::<ReleaseInfo>()
        .context("parse latest release json")
}

/// 平台关键字，对齐 CI 产物命名：
/// - Linux:   `Linux_amd64` / `Linux_arm64`
/// - Windows: `Win64` / `WinArm64`
/// - macOS:   `macOS_amd64` / `macOS_arm64`
fn detect_platform_keyword() -> Result<String> {
    let system = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let arch_key = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };

    match system {
        "linux" => Ok(format!("Linux_{arch_key}")),
        "windows" => match arch_key {
            "arm64" => Ok("WinArm64".to_string()),
            _ => Ok("Win64".to_string()),
        },
        "macos" => Ok(format!("macOS_{arch_key}")),
        other => Ok(other.to_string()),
    }
}

fn get_latest_release_asset() -> Result<MatchedAsset> {
    let client = build_http_client()?;
    let latest = fetch_latest_release(&client)?;

    let platform_key = detect_platform_keyword()?;
    let release_name = latest.name.unwrap_or_default();
    let tag_name = latest.tag_name.unwrap_or_default();

    if tag_name.is_empty() {
        return Err(anyhow!("latest release missing tag_name"));
    }

    for asset in latest.assets {
        // 跳过 Android APK（桌面端不需要）
        if asset.name.ends_with(".apk") {
            continue;
        }
        if asset.name.contains(&platform_key) {
            let original_url = asset.browser_download_url;
            let accel_disabled = std::env::var("RER_DISABLE_ACCEL").ok().as_deref() == Some("1");
            let download_url = if accel_disabled {
                original_url.clone()
            } else {
                get_accelerated_url(&original_url)
            };

            let sha256 = asset
                .digest
                .as_deref()
                .and_then(|d| d.split(':').next_back())
                .map(|s| s.trim().to_string())
                .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()));

            return Ok(MatchedAsset {
                release_name,
                tag_name,
                download_url,
                size: asset.size.unwrap_or(0),
                sha256,
            });
        }
    }

    Err(anyhow!(
        "no matching release asset for platform_key={platform_key}"
    ))
}

fn get_accelerated_url(original_url: &str) -> String {
    // https://github.com/<owner>/<repo>/releases/download/<tag>/<asset>
    // → https://dl.zhongbai233.com/release/<tag>/<asset>
    if let Some(tail) = original_url.split("/releases/download/").nth(1) {
        format!("https://dl.zhongbai233.com/release/{tail}")
    } else {
        original_url.to_string()
    }
}

fn start_update(matched: &MatchedAsset, on_progress: Option<ProgressCallback>) -> Result<()> {
    let tmp_dir = TempDir::new().context("create temp dir")?;
    let tmp_file = download_and_verify(tmp_dir.path(), matched, on_progress)?;

    if cfg!(windows) {
        windows_apply_and_restart(&tmp_file)?;
        std::process::exit(0);
    }

    let new_exe = unix_apply(&tmp_file)?;

    let mut cmd = Command::new(&new_exe);
    cmd.args(std::env::args_os().skip(1));
    cmd.spawn().context("spawn new executable")?;
    std::process::exit(0);
}

fn download_and_verify(
    tmp_dir: &Path,
    matched: &MatchedAsset,
    on_progress: Option<ProgressCallback>,
) -> Result<PathBuf> {
    let client = build_http_client()?;
    let url = &matched.download_url;

    let resp = client
        .get(url)
        .header(USER_AGENT, "Mozilla/5.0 RustEpubReader-Updater/1.0")
        .timeout(Duration::from_secs(300))
        .send()
        .with_context(|| format!("download asset: {url}"))?
        .error_for_status()
        .context("download status")?;

    let total = resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .or(Some(matched.size))
        .unwrap_or(0);

    let fname = Path::new(url)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("update.bin");

    let out_path = tmp_dir.join(fname);

    let mut hasher = Sha256::new();
    let mut file = fs::File::create(&out_path).context("create temp file")?;
    let mut reader = resp;
    let mut buf = [0u8; 8192];
    let mut downloaded: u64 = 0;

    loop {
        let n = reader.read(&mut buf).context("read download stream")?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).context("write temp file")?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        if let Some(ref cb) = on_progress {
            cb(downloaded, total);
        }
    }

    let actual = hex::encode(hasher.finalize());
    if let Some(expected) = matched.sha256.as_deref() {
        if !eq_hash(&actual, expected) {
            let _ = fs::remove_file(&out_path);
            return Err(anyhow!(
                "SHA256 校验失败：下载文件 {} 的哈希 {} 与期望 {} 不符",
                out_path.display(),
                actual,
                expected
            ));
        }
    }

    Ok(out_path)
}

fn current_executable_path() -> Result<PathBuf> {
    std::env::current_exe().context("current_exe")
}

fn compute_file_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).context("read file")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// 生成规范可执行文件名（不带版本号），对齐发行资产的平台关键字。
/// 例如：`RustEpubReader-Win64.exe`、`RustEpubReader-Linux_amd64`
fn canonical_executable_name() -> Result<OsString> {
    let platform_key = detect_platform_keyword()?;
    let mut name = format!("RustEpubReader-{platform_key}");
    if cfg!(windows) {
        name.push_str(".exe");
    }
    Ok(OsString::from(name))
}

fn target_executable_path() -> Result<PathBuf> {
    let local_exe = current_executable_path()?;
    let parent = local_exe
        .parent()
        .ok_or_else(|| anyhow!("cannot determine executable directory"))?;
    Ok(parent.join(canonical_executable_name()?))
}

fn move_or_copy(src: &Path, dst: &Path) -> Result<()> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(rename_err) => {
            fs::copy(src, dst).with_context(|| {
                format!(
                    "copy {} -> {} (rename failed: {})",
                    src.display(),
                    dst.display(),
                    rename_err
                )
            })?;
            let _ = fs::remove_file(src);
            Ok(())
        }
    }
}

fn unix_apply(tmp_file: &Path) -> Result<PathBuf> {
    let local_exe = current_executable_path()?;
    let target_exe = target_executable_path()?;
    let staged = {
        let parent = target_exe
            .parent()
            .ok_or_else(|| anyhow!("cannot determine executable directory"))?;
        let mut name = OsString::from(
            target_exe
                .file_name()
                .ok_or_else(|| anyhow!("invalid target executable name"))?,
        );
        name.push(".new");
        parent.join(name)
    };
    let _ = fs::remove_file(&staged);

    move_or_copy(tmp_file, &staged).context("stage new executable")?;

    // Unix rename 可原子覆盖目标文件
    fs::rename(&staged, &target_exe).context("replace target executable")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = fs::metadata(&target_exe)?.permissions();
        perm.set_mode(0o755);
        let _ = fs::set_permissions(&target_exe, perm);
    }

    // 若旧文件名与目标文件名不同（如带版本号），删除旧文件
    if local_exe != target_exe {
        let _ = fs::remove_file(&local_exe);
    }

    Ok(target_exe)
}

fn windows_apply_and_restart(tmp_file: &Path) -> Result<()> {
    let local_exe = current_executable_path()?;
    let parent = local_exe
        .parent()
        .ok_or_else(|| anyhow!("cannot determine executable directory"))?;

    let exe_name = local_exe
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("invalid exe name"))?;

    let target_name = canonical_executable_name()?;
    let target_name = target_name
        .to_str()
        .ok_or_else(|| anyhow!("invalid target exe name"))?
        .to_string();

    // stage 到目标目录
    let staged = parent.join(format!("{target_name}.new"));
    let _ = fs::remove_file(&staged);
    move_or_copy(tmp_file, &staged).context("stage new executable")?;

    let args: Vec<OsString> = std::env::args_os().skip(1).collect();

    // SECURITY: The batch script only uses paths derived from the canonical executable name
    // (validated above) and the parent directory of the current exe. User-supplied arguments
    // are NOT interpolated into the script body; they are passed via cmd.exe args.
    let mut lines = Vec::new();
    lines.push("@echo off".to_string());
    lines.push("echo Updating RustEpubReader, please wait...".to_string());
    lines.push("timeout /t 3 /nobreak".to_string());
    lines.push(String::new());
    lines.push(format!("cd /d \"{}\"", parent.display()));
    lines.push(String::new());
    lines.push(format!(
        "if exist \"{}\" (del /F /Q \"{}\")",
        exe_name, exe_name
    ));
    if target_name != exe_name {
        lines.push(format!(
            "if exist \"{}\" (del /F /Q \"{}\")",
            target_name, target_name
        ));
    }
    lines.push(format!(
        "if exist \"{target_name}.new\" (ren \"{target_name}.new\" \"{target_name}\")"
    ));
    lines.push(String::new());
    lines.push(format!("start \"\" \"{}\" %*", target_name));
    lines.push(String::new());
    lines.push("del \"%~f0\"".to_string());

    let bat_content = lines.join("\r\n");
    let bat_path = std::env::temp_dir().join("rer_update_script.bat");
    fs::write(&bat_path, &bat_content).context("write update bat")?;

    Command::new("cmd")
        .args(["/C", bat_path.to_string_lossy().as_ref()])
        .args(args)
        .spawn()
        .context("spawn update bat")?;

    Ok(())
}
