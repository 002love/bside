use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[allow(dead_code)]
pub const LAUNCHD_LABEL: &str = "dev.bside.watch";
#[allow(dead_code)]
pub const SYSTEMD_SERVICE_NAME: &str = "bside.service";
#[allow(dead_code)]
pub const WINDOWS_TASK_NAME: &str = "bside-watch";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    pub binary_path: PathBuf,
    pub cache_dir: Option<PathBuf>,
    pub db_path: Option<PathBuf>,
    pub log_path: PathBuf,
    pub friends: Vec<String>,
    pub interval: u64,
    pub profile_interval: u64,
    pub max_inferred_repeats: u32,
}

/// Returns the default log file path (`<data_local_dir>/bside/bside.log`).
pub fn default_log_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("bside")
        .join("bside.log")
}

/// Returns the macOS LaunchAgent plist path (`~/Library/LaunchAgents/dev.bside.watch.plist`).
#[allow(dead_code)]
pub fn launchd_plist_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist")))
}

/// Returns the Linux systemd user unit path (`~/.config/systemd/user/bside.service`).
#[allow(dead_code)]
pub fn systemd_unit_path() -> Result<PathBuf> {
    let config_base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .context("Could not determine user config directory")?;
    Ok(config_base
        .join("systemd")
        .join("user")
        .join(SYSTEMD_SERVICE_NAME))
}

/// Builds the exact CLI argument list for running `bside watch`.
pub fn build_watch_args(config: &ServiceConfig) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(cache_dir) = &config.cache_dir {
        args.push("--cache-dir".to_string());
        args.push(cache_dir.to_string_lossy().into_owned());
    }
    if let Some(db_path) = &config.db_path {
        args.push("--db".to_string());
        args.push(db_path.to_string_lossy().into_owned());
    }
    args.push("watch".to_string());
    for friend in &config.friends {
        args.push("--friend".to_string());
        args.push(friend.clone());
    }
    args.push("--interval".to_string());
    args.push(config.interval.to_string());
    args.push("--profile-interval".to_string());
    args.push(config.profile_interval.to_string());
    args.push("--max-inferred-repeats".to_string());
    args.push(config.max_inferred_repeats.to_string());
    args
}

/// Escapes special XML characters for safe inclusion in a plist `<string>` element.
#[allow(dead_code)]
fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Renders a macOS LaunchAgent `.plist` XML document for running `bside watch`.
#[allow(dead_code)]
pub fn render_launchd_plist(config: &ServiceConfig) -> String {
    let mut program_args = Vec::new();
    program_args.push(config.binary_path.to_string_lossy().into_owned());
    program_args.extend(build_watch_args(config));

    let args_xml = program_args
        .iter()
        .map(|arg| format!("        <string>{}</string>", xml_escape(arg)))
        .collect::<Vec<_>>()
        .join("\n");

    let log_path = xml_escape(&config.log_path.to_string_lossy());

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LAUNCHD_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
{args_xml}
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log_path}</string>
    <key>StandardErrorPath</key>
    <string>{log_path}</string>
</dict>
</plist>
"#
    )
}

/// Quotes and escapes an argument for a systemd `ExecStart=` line.
#[allow(dead_code)]
fn quote_systemd_arg(arg: &str) -> String {
    let escaped_specifiers = arg.replace('%', "%%");
    let needs_quotes = escaped_specifiers.is_empty()
        || escaped_specifiers.chars().any(|c| {
            c.is_whitespace() || matches!(c, '"' | '\'' | '\\' | ';' | '$' | '#' | '&' | '|' | '`' | '(' | ')')
        });

    if !needs_quotes {
        return escaped_specifiers;
    }

    let mut quoted = String::with_capacity(escaped_specifiers.len() + 2);
    quoted.push('"');
    for ch in escaped_specifiers.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

/// Renders a Linux `systemd --user` service unit file for running `bside watch`.
#[allow(dead_code)]
pub fn render_systemd_unit(config: &ServiceConfig) -> String {
    let mut exec_parts = Vec::new();
    exec_parts.push(quote_systemd_arg(&config.binary_path.to_string_lossy()));
    for arg in build_watch_args(config) {
        exec_parts.push(quote_systemd_arg(&arg));
    }
    let exec_start = exec_parts.join(" ");
    let log_path = config.log_path.to_string_lossy().replace('%', "%%");

    format!(
        r#"[Unit]
Description=bside Spotify Friend Activity Watcher
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={exec_start}
Restart=on-failure
RestartSec=15
StandardOutput=append:{log_path}
StandardError=append:{log_path}

[Install]
WantedBy=default.target
"#
    )
}

/// Quotes an argument for use inside a Windows `cmd /C` command line.
#[allow(dead_code)]
fn quote_windows_arg(arg: &str) -> String {
    let needs_quotes = arg.is_empty()
        || arg.chars().any(|c| {
            c.is_whitespace()
                || matches!(c, '"' | '&' | '|' | '<' | '>' | '^' | '%' | '(' | ')')
        });

    if !needs_quotes {
        return arg.to_string();
    }

    let escaped = arg.replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Builds arguments for Windows `schtasks.exe` to register the `bside-watch` logon task.
#[allow(dead_code)]
pub fn build_windows_schtasks_args(config: &ServiceConfig) -> Vec<String> {
    let bin = quote_windows_arg(&config.binary_path.to_string_lossy());
    let watch_args = build_watch_args(config)
        .into_iter()
        .map(|a| quote_windows_arg(&a))
        .collect::<Vec<_>>()
        .join(" ");
    let log_path = quote_windows_arg(&config.log_path.to_string_lossy());

    let tr_command = format!("cmd /C \"{bin} {watch_args} >> {log_path} 2>&1\"");

    vec![
        "/Create".to_string(),
        "/F".to_string(),
        "/SC".to_string(),
        "ONLOGON".to_string(),
        "/RL".to_string(),
        "LIMITED".to_string(),
        "/TN".to_string(),
        WINDOWS_TASK_NAME.to_string(),
        "/TR".to_string(),
        tr_command,
    ]
}

fn has_cached_credentials(cache_dir: &Path) -> bool {
    if !cache_dir.exists() {
        return false;
    }
    crate::spotify::open_cache(cache_dir)
        .ok()
        .and_then(|cache| cache.credentials())
        .is_some()
}

/// Returns up to the last `max_lines` lines from `content`.
fn tail_lines(content: &str, max_lines: usize) -> Vec<&str> {
    if max_lines == 0 {
        return Vec::new();
    }
    let all: Vec<&str> = content.lines().collect();
    let start = all.len().saturating_sub(max_lines);
    all[start..].to_vec()
}

/// Installs, enables, and starts the background `bside watch` service.
pub fn install(config: &ServiceConfig, resolved_cache_dir: &Path) -> Result<()> {
    if !has_cached_credentials(resolved_cache_dir) {
        eprintln!(
            "Warning: No cached Spotify credentials found in {}. Run `bside login` first so the background service can authenticate.",
            resolved_cache_dir.display()
        );
    }

    if let Some(parent) = config.log_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create log directory {}", parent.display())
        })?;
    }

    platform::install(config)
}

/// Stops and removes the background `bside watch` service and its definition file.
pub fn uninstall() -> Result<()> {
    platform::uninstall()
}

/// Starts the installed background `bside watch` service.
pub fn start() -> Result<()> {
    platform::start()
}

/// Stops the running background `bside watch` service.
pub fn stop() -> Result<()> {
    platform::stop()
}

/// Reports the live status of the background `bside watch` service.
pub fn status() -> Result<()> {
    platform::status()
}

/// Reads and prints the last `lines` lines from the service log file.
pub fn logs(log_path: &Path, lines: usize) -> Result<()> {
    if !log_path.exists() {
        println!(
            "No log file found at {}. Install and start the service with `bside service install`.",
            log_path.display()
        );
        return Ok(());
    }

    let content = fs::read_to_string(log_path)
        .with_context(|| format!("Failed to read log file {}", log_path.display()))?;

    let trailing = tail_lines(&content, lines);
    if trailing.is_empty() {
        println!("Log file at {} is empty.", log_path.display());
        return Ok(());
    }

    for line in trailing {
        println!("{line}");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod platform {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    use anyhow::{Context, Result, bail};

    use super::{
        LAUNCHD_LABEL, ServiceConfig, default_log_path, launchd_plist_path, render_launchd_plist,
    };

    fn current_uid() -> Result<String> {
        let output = Command::new("id")
            .arg("-u")
            .output()
            .context("Failed to execute `id -u`")?;
        if !output.status.success() {
            bail!("Failed to determine current user UID via `id -u`");
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn gui_domain() -> Result<String> {
        Ok(format!("gui/{}", current_uid()?))
    }

    fn service_target() -> Result<String> {
        Ok(format!("{}/{LAUNCHD_LABEL}", gui_domain()?))
    }

    fn bootout_if_loaded(plist_path: &Path) {
        if let Ok(domain) = gui_domain() {
            let _ = Command::new("launchctl")
                .args(["bootout", &domain])
                .arg(plist_path)
                .output();
            let _ = Command::new("launchctl")
                .args(["bootout", &format!("{domain}/{LAUNCHD_LABEL}")])
                .output();
        }
        let _ = Command::new("launchctl")
            .args(["unload", "-w"])
            .arg(plist_path)
            .output();
    }

    fn bootstrap_service(plist_path: &Path) -> Result<()> {
        let domain = gui_domain()?;
        let out = Command::new("launchctl")
            .args(["bootstrap", &domain])
            .arg(plist_path)
            .output()
            .context("Failed to run `launchctl bootstrap`")?;

        if out.status.success() {
            return Ok(());
        }

        // Fall back to `launchctl load -w` for older macOS environments.
        let fallback = Command::new("launchctl")
            .args(["load", "-w"])
            .arg(plist_path)
            .output()
            .context("Failed to run `launchctl load`")?;

        if fallback.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&out.stderr);
        let fallback_stderr = String::from_utf8_lossy(&fallback.stderr);
        bail!(
            "Failed to load LaunchAgent: {} (fallback: {})",
            stderr.trim(),
            fallback_stderr.trim()
        );
    }

    pub fn install(config: &ServiceConfig) -> Result<()> {
        let plist_path = launchd_plist_path()?;
        if let Some(parent) = plist_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create LaunchAgents directory {}", parent.display())
            })?;
        }

        // Stop any existing instance before overwriting the plist.
        bootout_if_loaded(&plist_path);

        let plist_xml = render_launchd_plist(config);
        fs::write(&plist_path, plist_xml)
            .with_context(|| format!("Failed to write LaunchAgent plist {}", plist_path.display()))?;

        bootstrap_service(&plist_path)?;

        println!("Installed and started macOS LaunchAgent ({LAUNCHD_LABEL}).");
        println!("  Plist: {}", plist_path.display());
        println!("  Logs:  {}", config.log_path.display());
        Ok(())
    }

    pub fn uninstall() -> Result<()> {
        let plist_path = launchd_plist_path()?;
        bootout_if_loaded(&plist_path);

        if plist_path.exists() {
            fs::remove_file(&plist_path).with_context(|| {
                format!("Failed to remove LaunchAgent plist {}", plist_path.display())
            })?;
            println!("Uninstalled macOS LaunchAgent ({LAUNCHD_LABEL}).");
            println!("  Removed: {}", plist_path.display());
        } else {
            println!("Service {LAUNCHD_LABEL} is not installed (no plist found at {}).", plist_path.display());
        }
        Ok(())
    }

    pub fn start() -> Result<()> {
        let plist_path = launchd_plist_path()?;
        if !plist_path.exists() {
            bail!(
                "Service plist not found at {}. Run `bside service install` first.",
                plist_path.display()
            );
        }

        let target = service_target()?;
        let kickstart = Command::new("launchctl")
            .args(["kickstart", "-k", &target])
            .output()
            .context("Failed to run `launchctl kickstart`")?;

        if !kickstart.status.success() {
            bootstrap_service(&plist_path)?;
        }

        println!("Started macOS LaunchAgent ({LAUNCHD_LABEL}).");
        Ok(())
    }

    pub fn stop() -> Result<()> {
        let plist_path = launchd_plist_path()?;
        let domain = gui_domain()?;
        let target = service_target()?;

        let out = Command::new("launchctl")
            .args(["bootout", &target])
            .output()
            .context("Failed to run `launchctl bootout`")?;

        if !out.status.success() && plist_path.exists() {
            let _ = Command::new("launchctl")
                .args(["bootout", &domain])
                .arg(&plist_path)
                .output();
            let _ = Command::new("launchctl")
                .args(["unload", "-w"])
                .arg(&plist_path)
                .output();
        }

        println!("Stopped macOS LaunchAgent ({LAUNCHD_LABEL}).");
        Ok(())
    }

    pub fn status() -> Result<()> {
        let plist_path = launchd_plist_path()?;
        let installed = plist_path.exists();
        let target = service_target()?;

        println!("Service:   {LAUNCHD_LABEL} (macOS launchd)");
        println!("Installed: {}", if installed { "yes" } else { "no" });
        println!("Plist:     {}", plist_path.display());
        println!("Logs:      {}", default_log_path().display());

        let out = Command::new("launchctl")
            .args(["print", &target])
            .output()
            .context("Failed to run `launchctl print`")?;

        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut state = "loaded";
            let mut pid: Option<String> = None;
            for line in stdout.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("state = ") {
                    state = if rest.contains("running") {
                        "running"
                    } else {
                        "loaded (not running)"
                    };
                } else if let Some(rest) = trimmed.strip_prefix("pid = ") {
                    pid = Some(rest.trim().to_string());
                }
            }
            if let Some(pid) = pid {
                println!("Status:    {state} (PID {pid})");
            } else {
                println!("Status:    {state}");
            }
        } else {
            println!("Status:    stopped");
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::fs;
    use std::process::Command;

    use anyhow::{Context, Result, bail};

    use super::{
        SYSTEMD_SERVICE_NAME, ServiceConfig, default_log_path, render_systemd_unit,
        systemd_unit_path,
    };

    fn run_systemctl(args: &[&str]) -> Result<std::process::Output> {
        Command::new("systemctl")
            .arg("--user")
            .args(args)
            .output()
            .with_context(|| format!("Failed to execute `systemctl --user {}`", args.join(" ")))
    }

    fn require_systemctl(args: &[&str]) -> Result<()> {
        let out = run_systemctl(args)?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!(
                "`systemctl --user {}` failed: {}",
                args.join(" "),
                stderr.trim()
            );
        }
        Ok(())
    }

    pub fn install(config: &ServiceConfig) -> Result<()> {
        let unit_path = systemd_unit_path()?;
        if let Some(parent) = unit_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create systemd user directory {}", parent.display())
            })?;
        }

        let unit_contents = render_systemd_unit(config);
        fs::write(&unit_path, unit_contents)
            .with_context(|| format!("Failed to write systemd unit {}", unit_path.display()))?;

        require_systemctl(&["daemon-reload"])?;
        require_systemctl(&["enable", "--now", SYSTEMD_SERVICE_NAME])?;

        println!("Installed and started Linux systemd user service ({SYSTEMD_SERVICE_NAME}).");
        println!("  Unit: {}", unit_path.display());
        println!("  Logs: {}", config.log_path.display());
        Ok(())
    }

    pub fn uninstall() -> Result<()> {
        let unit_path = systemd_unit_path()?;
        let _ = run_systemctl(&["disable", "--now", SYSTEMD_SERVICE_NAME]);

        if unit_path.exists() {
            fs::remove_file(&unit_path)
                .with_context(|| format!("Failed to remove systemd unit {}", unit_path.display()))?;
            let _ = run_systemctl(&["daemon-reload"]);
            println!("Uninstalled Linux systemd user service ({SYSTEMD_SERVICE_NAME}).");
            println!("  Removed: {}", unit_path.display());
        } else {
            println!(
                "Service {SYSTEMD_SERVICE_NAME} is not installed (no unit found at {}).",
                unit_path.display()
            );
        }
        Ok(())
    }

    pub fn start() -> Result<()> {
        let unit_path = systemd_unit_path()?;
        if !unit_path.exists() {
            bail!(
                "Service unit not found at {}. Run `bside service install` first.",
                unit_path.display()
            );
        }
        require_systemctl(&["start", SYSTEMD_SERVICE_NAME])?;
        println!("Started Linux systemd user service ({SYSTEMD_SERVICE_NAME}).");
        Ok(())
    }

    pub fn stop() -> Result<()> {
        require_systemctl(&["stop", SYSTEMD_SERVICE_NAME])?;
        println!("Stopped Linux systemd user service ({SYSTEMD_SERVICE_NAME}).");
        Ok(())
    }

    pub fn status() -> Result<()> {
        let unit_path = systemd_unit_path()?;
        let installed = unit_path.exists();

        println!("Service:   {SYSTEMD_SERVICE_NAME} (systemd --user)");
        println!("Installed: {}", if installed { "yes" } else { "no" });
        println!("Unit:      {}", unit_path.display());
        println!("Logs:      {}", default_log_path().display());

        let out = run_systemctl(&["status", SYSTEMD_SERVICE_NAME, "--no-pager"])?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stdout.trim().is_empty() {
            println!("\n{}", stdout.trim_end());
        } else if !stderr.trim().is_empty() {
            println!("Status:    {}", stderr.trim());
        } else {
            println!("Status:    unknown");
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::process::Command;

    use anyhow::{Context, Result, bail};

    use super::{
        ServiceConfig, WINDOWS_TASK_NAME, build_windows_schtasks_args, default_log_path,
    };

    fn run_schtasks(args: &[String]) -> Result<std::process::Output> {
        Command::new("schtasks")
            .args(args)
            .output()
            .with_context(|| format!("Failed to execute `schtasks {}`", args.join(" ")))
    }

    pub fn install(config: &ServiceConfig) -> Result<()> {
        let create_args = build_windows_schtasks_args(config);
        let out = run_schtasks(&create_args)?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            bail!(
                "Failed to register Windows scheduled task: {} {}",
                stdout.trim(),
                stderr.trim()
            );
        }

        let run_args = vec![
            "/Run".to_string(),
            "/TN".to_string(),
            WINDOWS_TASK_NAME.to_string(),
        ];
        let run_out = run_schtasks(&run_args)?;
        if !run_out.status.success() {
            let stderr = String::from_utf8_lossy(&run_out.stderr);
            bail!("Task registered, but failed to start: {}", stderr.trim());
        }

        println!("Installed and started Windows Task Scheduler task ({WINDOWS_TASK_NAME}).");
        println!("  Logs: {}", config.log_path.display());
        Ok(())
    }

    pub fn uninstall() -> Result<()> {
        let _ = run_schtasks(&[
            "/End".to_string(),
            "/TN".to_string(),
            WINDOWS_TASK_NAME.to_string(),
        ]);

        let out = run_schtasks(&[
            "/Delete".to_string(),
            "/TN".to_string(),
            WINDOWS_TASK_NAME.to_string(),
            "/F".to_string(),
        ])?;

        if out.status.success() {
            println!("Uninstalled Windows Task Scheduler task ({WINDOWS_TASK_NAME}).");
        } else {
            println!("Service {WINDOWS_TASK_NAME} is not installed.");
        }
        Ok(())
    }

    pub fn start() -> Result<()> {
        let out = run_schtasks(&[
            "/Run".to_string(),
            "/TN".to_string(),
            WINDOWS_TASK_NAME.to_string(),
        ])?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!(
                "Failed to start Windows task {WINDOWS_TASK_NAME}: {}",
                stderr.trim()
            );
        }
        println!("Started Windows Task Scheduler task ({WINDOWS_TASK_NAME}).");
        Ok(())
    }

    pub fn stop() -> Result<()> {
        let out = run_schtasks(&[
            "/End".to_string(),
            "/TN".to_string(),
            WINDOWS_TASK_NAME.to_string(),
        ])?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!(
                "Failed to stop Windows task {WINDOWS_TASK_NAME}: {}",
                stderr.trim()
            );
        }
        println!("Stopped Windows Task Scheduler task ({WINDOWS_TASK_NAME}).");
        Ok(())
    }

    pub fn status() -> Result<()> {
        println!("Service: {WINDOWS_TASK_NAME} (Windows Task Scheduler)");
        println!("Logs:    {}", default_log_path().display());

        let out = run_schtasks(&[
            "/Query".to_string(),
            "/TN".to_string(),
            WINDOWS_TASK_NAME.to_string(),
            "/V".to_string(),
            "/FO".to_string(),
            "LIST".to_string(),
        ])?;

        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            println!("Installed: yes");
            println!("\n{}", stdout.trim());
        } else {
            println!("Installed: no");
            println!("Status:    not installed");
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod platform {
    use anyhow::{Result, bail};

    use super::ServiceConfig;

    pub fn install(_config: &ServiceConfig) -> Result<()> {
        bail!("Background service management is not supported on this operating system.")
    }

    pub fn uninstall() -> Result<()> {
        bail!("Background service management is not supported on this operating system.")
    }

    pub fn start() -> Result<()> {
        bail!("Background service management is not supported on this operating system.")
    }

    pub fn stop() -> Result<()> {
        bail!("Background service management is not supported on this operating system.")
    }

    pub fn status() -> Result<()> {
        bail!("Background service management is not supported on this operating system.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> ServiceConfig {
        ServiceConfig {
            binary_path: PathBuf::from("/usr/local/bin/bside"),
            cache_dir: None,
            db_path: None,
            log_path: PathBuf::from("/home/alice/.local/share/bside/bside.log"),
            friends: vec![],
            interval: 15,
            profile_interval: 1800,
            max_inferred_repeats: 10,
        }
    }

    #[test]
    fn build_watch_args_defaults_without_optional_paths() {
        let cfg = sample_config();
        let args = build_watch_args(&cfg);
        assert_eq!(
            args,
            vec![
                "watch",
                "--interval",
                "15",
                "--profile-interval",
                "1800",
                "--max-inferred-repeats",
                "10",
            ]
        );
    }

    #[test]
    fn build_watch_args_includes_optional_paths_and_multiple_friends() {
        let cfg = ServiceConfig {
            binary_path: PathBuf::from("/opt/bside/bside"),
            cache_dir: Some(PathBuf::from("/tmp/bside cache")),
            db_path: Some(PathBuf::from("/tmp/my history.db")),
            log_path: PathBuf::from("/tmp/bside.log"),
            friends: vec!["Alice".to_string(), "Bob & Carol".to_string()],
            interval: 30,
            profile_interval: 600,
            max_inferred_repeats: 5,
        };
        let args = build_watch_args(&cfg);
        assert_eq!(
            args,
            vec![
                "--cache-dir",
                "/tmp/bside cache",
                "--db",
                "/tmp/my history.db",
                "watch",
                "--friend",
                "Alice",
                "--friend",
                "Bob & Carol",
                "--interval",
                "30",
                "--profile-interval",
                "600",
                "--max-inferred-repeats",
                "5",
            ]
        );
    }

    #[test]
    fn render_launchd_plist_contains_required_keys_and_escapes_xml() {
        let cfg = ServiceConfig {
            binary_path: PathBuf::from("/Users/a&b/bin/bside"),
            cache_dir: Some(PathBuf::from("/Users/a&b/cache/<dev>")),
            db_path: None,
            log_path: PathBuf::from("/Users/a&b/Logs/bside's \"log\".log"),
            friends: vec!["Tom & Jerry".to_string(), "O'Connor".to_string()],
            interval: 20,
            profile_interval: 900,
            max_inferred_repeats: 8,
        };

        let plist = render_launchd_plist(&cfg);

        assert!(plist.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
        assert!(plist.contains("<key>Label</key>\n    <string>dev.bside.watch</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
        assert!(plist.contains("<key>KeepAlive</key>\n    <true/>"));
        assert!(plist.contains("<string>/Users/a&amp;b/bin/bside</string>"));
        assert!(plist.contains("<string>/Users/a&amp;b/cache/&lt;dev&gt;</string>"));
        assert!(plist.contains("<string>Tom &amp; Jerry</string>"));
        assert!(plist.contains("<string>O&apos;Connor</string>"));
        assert!(plist.contains(
            "<key>StandardOutPath</key>\n    <string>/Users/a&amp;b/Logs/bside&apos;s &quot;log&quot;.log</string>"
        ));
        assert!(plist.contains(
            "<key>StandardErrorPath</key>\n    <string>/Users/a&amp;b/Logs/bside&apos;s &quot;log&quot;.log</string>"
        ));
    }

    #[test]
    fn render_systemd_unit_formats_and_quotes_arguments() {
        let cfg = ServiceConfig {
            binary_path: PathBuf::from("/home/alice/my apps/bside"),
            cache_dir: Some(PathBuf::from("/home/alice/cache 100%")),
            db_path: Some(PathBuf::from("/home/alice/db/history.db")),
            log_path: PathBuf::from("/home/alice/.local/share/bside/bside%log.log"),
            friends: vec!["Alice".to_string(), "Bob \"DJ\" Smith".to_string()],
            interval: 15,
            profile_interval: 1800,
            max_inferred_repeats: 10,
        };

        let unit = render_systemd_unit(&cfg);

        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("[Service]"));
        assert!(unit.contains("[Install]\nWantedBy=default.target"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(unit.contains("RestartSec=15"));
        assert!(unit.contains(
            r#"ExecStart="/home/alice/my apps/bside" --cache-dir "/home/alice/cache 100%%" --db /home/alice/db/history.db watch --friend Alice --friend "Bob \"DJ\" Smith" --interval 15 --profile-interval 1800 --max-inferred-repeats 10"#
        ));
        assert!(unit.contains(
            "StandardOutput=append:/home/alice/.local/share/bside/bside%%log.log"
        ));
        assert!(unit.contains(
            "StandardError=append:/home/alice/.local/share/bside/bside%%log.log"
        ));
    }

    #[test]
    fn build_windows_schtasks_args_produces_expected_command() {
        let cfg = ServiceConfig {
            binary_path: PathBuf::from(r"C:\Program Files\bside\bside.exe"),
            cache_dir: None,
            db_path: Some(PathBuf::from(r"C:\Users\Alice\AppData\Local\bside\history.db")),
            log_path: PathBuf::from(r"C:\Users\Alice\AppData\Local\bside\bside.log"),
            friends: vec!["Alice Smith".to_string()],
            interval: 15,
            profile_interval: 1800,
            max_inferred_repeats: 10,
        };

        let args = build_windows_schtasks_args(&cfg);

        assert_eq!(args[0..9], [
            "/Create",
            "/F",
            "/SC",
            "ONLOGON",
            "/RL",
            "LIMITED",
            "/TN",
            "bside-watch",
            "/TR",
        ]);
        assert_eq!(
            args[9],
            r#"cmd /C ""C:\Program Files\bside\bside.exe" --db C:\Users\Alice\AppData\Local\bside\history.db watch --friend "Alice Smith" --interval 15 --profile-interval 1800 --max-inferred-repeats 10 >> C:\Users\Alice\AppData\Local\bside\bside.log 2>&1""#
        );
    }

    #[test]
    fn tail_lines_returns_trailing_lines() {
        let text = "line1\nline2\nline3\nline4\n";
        assert_eq!(tail_lines(text, 2), vec!["line3", "line4"]);
        assert_eq!(tail_lines(text, 10), vec!["line1", "line2", "line3", "line4"]);
        assert!(tail_lines(text, 0).is_empty());
        assert!(tail_lines("", 5).is_empty());
    }
}
