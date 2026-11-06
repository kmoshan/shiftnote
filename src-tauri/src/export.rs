//! 预览和文件导出使用同一文本生成器。先完整写入同目录临时文件，再原子发布。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};

use crate::db::handovers::SECTIONS;
use crate::error::{AppError, AppResult};
use crate::models::{HandoverDetail, HandoverExportResult};

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;

fn escape(value: &str, markdown: bool) -> String {
    if !markdown {
        return value.to_string();
    }
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.'
            | '!' | '|' | '~' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn normalized(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn field(lines: &mut Vec<String>, label: &str, value: &str, markdown: bool) {
    for (index, line) in normalized(value).split('\n').enumerate() {
        lines.push(format!(
            "{}{}",
            if index == 0 { label } else { "    " },
            escape(line, markdown)
        ));
    }
}

fn timestamp(ms: i64) -> AppResult<String> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .ok_or_else(|| AppError::validation("快照包含无效的时间戳"))
}

pub fn render(detail: &HandoverDetail, format: &str) -> AppResult<String> {
    let markdown = match format {
        "markdown" | "md" => true,
        "txt" | "text" => false,
        _ => return Err(AppError::validation("仅支持 Markdown 或 TXT 格式")),
    };
    if detail
        .items
        .iter()
        .any(|item| !SECTIONS.iter().any(|(key, _)| *key == item.section))
    {
        return Err(AppError::validation("交接包含未知分区，无法完整生成文本"));
    }
    let h = &detail.handover;
    let mut lines = Vec::new();
    field(
        &mut lines,
        if markdown { "# " } else { "" },
        &normalized(&h.title).replace('\n', " "),
        markdown,
    );
    lines.push(String::new());
    field(
        &mut lines,
        "交接区间：",
        &format!(
            "{} 至 {}（结束时间不包含）",
            timestamp(h.period_start)?,
            timestamp(h.period_end)?
        ),
        markdown,
    );
    if let Some(value) = &h.shift_from {
        field(&mut lines, "交班：", value, markdown);
    }
    if let Some(value) = &h.shift_to {
        field(&mut lines, "接班：", value, markdown);
    }
    if let Some(value) = &h.note {
        field(&mut lines, "交接备注：", value, markdown);
    }
    for (section, title) in SECTIONS {
        lines.push(String::new());
        lines.push(format!("{}{title}", if markdown { "## " } else { "" }));
        lines.push(String::new());
        let mut items = detail
            .items
            .iter()
            .filter(|item| item.section == *section)
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.id.cmp(&b.id)));
        if items.is_empty() {
            lines.push("（无）".into());
        }
        for item in items {
            let task = &item.snapshot.task;
            field(&mut lines, "- ", &task.title, markdown);
            let status = match task.status.as_str() {
                "open" => "待处理",
                "done" => "已处理",
                _ => return Err(AppError::validation("快照包含无效的事项状态")),
            };
            field(
                &mut lines,
                "  优先级 / 状态：",
                &format!("P{} / {status}", task.priority),
                markdown,
            );
            if let Some(department) = &task.department {
                field(&mut lines, "  负责部门：", department, markdown);
            }
            field(
                &mut lines,
                "  记录时间：",
                &timestamp(task.recorded_at)?,
                markdown,
            );
            field(&mut lines, "  事项 ID：", &task.id, markdown);
            if let Some(description) = &task.description {
                field(&mut lines, "  描述：", description, markdown);
            }
            if let Some(note) = &item.note {
                field(&mut lines, "  交接备注：", note, markdown);
            }
            let mut updates = item.snapshot.updates.iter().collect::<Vec<_>>();
            updates.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
            if updates.is_empty() {
                lines.push("  本次未选择进展。".into());
            }
            for update in updates {
                let author = update.author.as_deref().unwrap_or("未记录处理人");
                field(
                    &mut lines,
                    "  进展时间 / 处理人：",
                    &format!("{} / {author}", timestamp(update.created_at)?),
                    markdown,
                );
                field(&mut lines, "    ", &update.content, markdown);
            }
            lines.push(String::new());
        }
    }
    // 所有来源先统一 LF，再一次性生成 CRLF；不加 BOM。
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    Ok(format!("{}\r\n", lines.join("\r\n")))
}

struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn conflict() -> AppError {
    AppError::Conflict("目标文件已存在，请确认后覆盖".into())
}

/// 只有定稿允许写入文件；预览可调用 render 查看实时草稿。
pub fn export(
    detail: &HandoverDetail,
    format: &str,
    path: &Path,
    overwrite: bool,
) -> AppResult<HandoverExportResult> {
    if detail.handover.status != "finalized" {
        return Err(AppError::validation("请先定稿，再导出交接记录"));
    }
    if !path.is_absolute() || path.file_name().is_none() {
        return Err(AppError::validation("请选择有效的绝对文件路径"));
    }
    let content = render(detail, format)?;
    if !overwrite && fs::symlink_metadata(path).is_ok() {
        return Err(conflict());
    }
    let parent = path
        .parent()
        .ok_or_else(|| AppError::validation("导出路径没有父目录"))?;
    let temp = TempFile(parent.join(format!(".shiftnote-export-{}.tmp", uuid::Uuid::new_v4())));
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp.0)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    publish(&temp.0, path, overwrite)?;
    Ok(HandoverExportResult {
        path: path.display().to_string(),
        bytes: content.len() as u64,
    })
}

#[cfg(windows)]
pub(crate) fn publish(source: &Path, target: &Path, overwrite: bool) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
        fn ReplaceFileW(
            replaced: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
    }
    let mut from = source.as_os_str().encode_wide().collect::<Vec<_>>();
    let mut to = target.as_os_str().encode_wide().collect::<Vec<_>>();
    if from.contains(&0) || to.contains(&0) {
        return Err(AppError::validation("导出路径包含无效字符"));
    }
    from.push(0);
    to.push(0);
    if overwrite {
        // ReplaceFile opens the old file for READ|DELETE, which also permits a
        // migration's READ|DELETE sharing lease to stay held across publication.
        // MoveFileEx alone can request write access and fail under that lease.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        loop {
            if unsafe {
                ReplaceFileW(
                    to.as_ptr(),
                    from.as_ptr(),
                    std::ptr::null(),
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            } != 0
            {
                return Ok(());
            }
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(2) {
                break;
            }
            // 共享冲突及 1175 保留原有文件名，可短暂重试；1176/1177 等
            // 不保证相同状态的错误立即返回，不删除目标或放宽共享/ACL。
            if matches!(error.raw_os_error(), Some(32 | 33 | 1175))
                && std::time::Instant::now() < deadline
            {
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }
            return Err(error.into());
        }
    }
    // MOVEFILE_WRITE_THROUGH；同目录移动，不启用跨卷 COPY_ALLOWED。
    let flags = 0x8 | if overwrite { 0x1 } else { 0 };
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), flags) } == 0 {
        let error = std::io::Error::last_os_error();
        if !overwrite && matches!(error.raw_os_error(), Some(80 | 183)) {
            return Err(conflict());
        }
        return Err(error.into());
    }
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn publish(source: &Path, target: &Path, overwrite: bool) -> AppResult<()> {
    if overwrite {
        fs::rename(source, target)?;
    } else {
        // 硬链接创建为原子不覆盖操作，避免 exists()+rename() 的竞争窗口。
        match fs::hard_link(source, target) {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(conflict()),
            other => other?,
        }
        // 发布已成功；临时链接清理由守卫重试，不能把清理失败误报为发布失败。
        let _ = fs::remove_file(source);
    }
    Ok(())
}
