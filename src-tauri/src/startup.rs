//! 启动失败时保留诊断信息，避免 Windows 发布版静默退出。

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Once;

static PANIC_HOOK: Once = Once::new();

/// 将非空路径解析为绝对路径；不要求目录已经存在。
pub fn absolute_data_dir(path: &str) -> io::Result<PathBuf> {
    let path = path.trim();
    if path.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "数据目录不能为空",
        ));
    }

    absolute_path(Path::new(path))
}

/// 自定义目录优先；有同名便携标记时使用程序旁的 data，否则使用标准目录。
pub fn data_dir_override(executable: &Path, custom: Option<&str>) -> io::Result<Option<PathBuf>> {
    if let Some(custom) = custom.filter(|value| !value.trim().is_empty()) {
        return absolute_data_dir(custom).map(Some);
    }
    if !executable.with_extension("portable").is_file() {
        return Ok(None);
    }
    let parent = executable
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "无法确定程序所在目录"))?;
    absolute_path(&parent.join("data")).map(Some)
}

fn absolute_path(path: &Path) -> io::Result<PathBuf> {
    #[cfg(windows)]
    {
        platform::absolute_path(path)
    }
    #[cfg(not(windows))]
    {
        use std::path::Component;

        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut normalized = PathBuf::new();
        for component in absolute.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                component => normalized.push(component.as_os_str()),
            }
        }
        Ok(normalized)
    }
}

/// 在应用入口安装一次；先保留日志和提示，再运行原有 panic hook。
pub fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let data_dir = std::env::var("SHIFTNOTE_DATA_DIR")
                .ok()
                .and_then(|path| absolute_data_dir(&path).ok());
            report_failure(&format!("程序发生异常：{info}"), data_dir.as_deref());
            previous(info);
        }));
    });
}

/// 追加启动日志，并向用户显示错误和实际日志路径。
pub fn report_failure(message: &str, data_dir: Option<&Path>) {
    let mut candidates = Vec::new();
    if let Some(data_dir) = data_dir {
        candidates.push(data_dir.to_path_buf());
    }
    candidates.push(std::env::temp_dir().join("ShiftNote"));
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.to_path_buf());
        }
    }

    let log_status = match append_startup_log(message, &candidates) {
        Ok(path) => format!("日志：{}", path.display()),
        Err(error) => format!("日志写入失败：{error}"),
    };
    let report = format!("ShiftNote 启动失败。\n\n{message}\n\n{log_status}");
    // stderr 不可用时也不能令 panic hook 再次 panic。
    let _ = writeln!(io::stderr().lock(), "{report}");

    #[cfg(windows)]
    if std::env::var("SHIFTNOTE_STARTUP_NO_DIALOG").as_deref() != Ok("1") {
        platform::show_error(&report);
    }
}

fn append_startup_log(message: &str, candidates: &[PathBuf]) -> io::Result<PathBuf> {
    let timestamp = chrono::Local::now().to_rfc3339();
    let mut failures = Vec::new();
    let mut last_kind = io::ErrorKind::NotFound;

    for directory in candidates {
        let path = directory.join("logs").join("startup.log");
        let result: io::Result<()> = (|| {
            std::fs::create_dir_all(path.parent().expect("日志路径有父目录"))?;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            let mut record = String::new();
            if !failures.is_empty() {
                record.push_str(&format!(
                    "[{timestamp}] 日志目录不可用，已回退：{}\n",
                    failures.join("；")
                ));
            }
            record.push_str(&format!("[{timestamp}] {message}\n"));
            file.write_all(record.as_bytes())?;
            Ok(())
        })();
        match result {
            Ok(()) => return Ok(path),
            Err(error) => {
                last_kind = error.kind();
                failures.push(format!("{}：{error}", path.display()));
            }
        }
    }

    Err(io::Error::new(
        last_kind,
        if failures.is_empty() {
            "没有可用的日志目录".to_string()
        } else {
            failures.join("；")
        },
    ))
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    #[link(name = "kernel32")]
    extern "system" {
        fn GetFullPathNameW(
            file_name: *const u16,
            buffer_length: u32,
            buffer: *mut u16,
            file_part: *mut *mut u16,
        ) -> u32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(
            window: *mut c_void,
            text: *const u16,
            caption: *const u16,
            kind: u32,
        ) -> i32;
    }

    pub(super) fn absolute_path(path: &Path) -> io::Result<PathBuf> {
        let mut path: Vec<u16> = path.as_os_str().encode_wide().collect();
        if path.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "数据目录不能包含空字符",
            ));
        }
        path.push(0);
        let mut buffer = vec![0u16; 260];
        loop {
            let length = unsafe {
                GetFullPathNameW(
                    path.as_ptr(),
                    buffer.len() as u32,
                    buffer.as_mut_ptr(),
                    std::ptr::null_mut(),
                )
            };
            if length == 0 {
                return Err(io::Error::last_os_error());
            }
            if (length as usize) < buffer.len() {
                buffer.truncate(length as usize);
                return Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer)));
            }
            buffer.resize(length as usize + 1, 0);
        }
    }

    pub(super) fn show_error(report: &str) {
        let text: Vec<u16> = report
            .replace('\0', "�")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let caption: Vec<u16> = "ShiftNote 启动失败".encode_utf16().chain(Some(0)).collect();
        // MB_OK | MB_ICONERROR
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::current_dir()
                .unwrap()
                .join("target")
                .join("startup-tests")
                .join(uuid::Uuid::new_v4().to_string());
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn absolute_data_directory_trims_and_normalizes_without_creating_it() {
        let unique = uuid::Uuid::new_v4();
        let name = format!("startup-not-created-{unique}");
        let path = absolute_data_dir(&format!("  ./startup-path-test/../{name}  ")).unwrap();
        assert!(path.is_absolute());
        assert_eq!(path, std::env::current_dir().unwrap().join(name));
        assert!(!path.exists());
        assert_eq!(absolute_data_dir(path.to_str().unwrap()).unwrap(), path);
        assert_eq!(
            absolute_data_dir(" \t\n").unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn custom_data_directory_takes_priority_over_portable_marker() {
        let directory = TestDirectory::new();
        let executable = directory.0.join("shiftnote.exe");
        std::fs::write(executable.with_extension("portable"), "").unwrap();
        let name = format!("custom-data-{}", uuid::Uuid::new_v4());

        let selected = data_dir_override(&executable, Some(&format!("  ./{name}  "))).unwrap();
        assert_eq!(selected, Some(std::env::current_dir().unwrap().join(name)));
        assert!(!directory.0.join("data").exists());
    }

    #[test]
    fn portable_marker_with_empty_custom_directory_uses_executable_directory() {
        let directory = TestDirectory::new();
        let executable = directory.0.join("portable app").join("shiftnote.exe");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(executable.with_extension("portable"), "").unwrap();
        let expected = executable.parent().unwrap().join("data");

        for custom in [None, Some(""), Some(" \t ")] {
            let selected = data_dir_override(&executable, custom).unwrap().unwrap();
            assert!(selected.is_absolute());
            assert_eq!(selected, expected);
        }
        assert!(!expected.exists());
    }

    #[test]
    fn absent_portable_marker_preserves_standard_directory_selection() {
        let directory = TestDirectory::new();
        let executable = directory.0.join("shiftnote.exe");
        for custom in [None, Some(""), Some(" \t ")] {
            assert_eq!(data_dir_override(&executable, custom).unwrap(), None);
        }
        // 同名目录不是标记文件，不能启用便携模式。
        std::fs::create_dir(executable.with_extension("portable")).unwrap();
        assert_eq!(data_dir_override(&executable, None).unwrap(), None);
    }

    #[test]
    fn log_append_preserves_previous_content() {
        let directory = TestDirectory::new();
        let log = directory.0.join("logs").join("startup.log");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "已有记录\n").unwrap();

        assert_eq!(
            append_startup_log("第一次错误", &[directory.0.clone()]).unwrap(),
            log
        );
        append_startup_log("第二次错误", &[directory.0.clone()]).unwrap();
        let content = std::fs::read_to_string(log).unwrap();
        assert!(content.starts_with("已有记录\n"));
        assert!(content.contains("第一次错误"));
        assert!(content.contains("第二次错误"));
        assert_eq!(content.lines().count(), 3);
        let timestamp = content.lines().nth(1).unwrap().split(']').next().unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(&timestamp[1..]).is_ok());
    }

    #[test]
    fn unavailable_preferred_directory_reports_its_fallback() {
        let directory = TestDirectory::new();
        let blocked = directory.0.join("blocked");
        std::fs::write(&blocked, "这里是文件，不能创建子目录").unwrap();
        let fallback = directory.0.join("fallback");

        let log = append_startup_log("启动失败", &[blocked.clone(), fallback.clone()]).unwrap();
        assert_eq!(log, fallback.join("logs").join("startup.log"));
        let content = std::fs::read_to_string(&log).unwrap();
        assert!(content.contains("已回退"));
        assert!(content.contains(&blocked.display().to_string()));
        assert!(content.contains("启动失败"));
        assert_eq!(
            std::fs::read_to_string(blocked).unwrap(),
            "这里是文件，不能创建子目录"
        );
    }
}
