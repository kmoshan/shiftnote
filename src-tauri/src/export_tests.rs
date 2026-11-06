use super::*;
use crate::db::{handovers, tasks};
use crate::models::*;
use rusqlite::Connection;

fn detail(finalized: bool) -> HandoverDetail {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    conn.execute_batch(include_str!("../migrations/001_init.sql"))
        .unwrap();
    conn.execute_batch(include_str!(
        "../migrations/002_department_and_recorded_at.sql"
    ))
    .unwrap();
    conn.execute_batch(include_str!("../migrations/003_handover_snapshots.sql"))
        .unwrap();
    let task = tasks::create(
        &conn,
        &CreateTaskInput {
            title: "中文 *标题* [链接](x) <tag>".into(),
            description: Some("首行\n# 次行".into()),
            department: Some("运维_一部".into()),
            recorded_at: Some(1000),
            ..Default::default()
        },
    )
    .unwrap();
    let draft = handovers::save(
        &conn,
        &HandoverSaveInput {
            id: None,
            title: "夜班 #1".into(),
            period_start: 1000,
            period_end: 3000,
            shift_from: Some("甲".into()),
            shift_to: Some("乙".into()),
            note: Some("备注\r\n- 特殊字符".into()),
            items: vec![HandoverItemInput {
                task_id: task.id,
                section: "focus".into(),
                note: Some("跟进\n第二行".into()),
                selected_update_ids: vec![],
            }],
        },
    )
    .unwrap();
    if finalized {
        handovers::finalize(&conn, &draft.handover.id).unwrap()
    } else {
        draft
    }
}

fn dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("shiftnote-export-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    dir
}

#[test]
fn markdown_escapes_chinese_content_and_uses_crlf_and_all_empty_sections() {
    let text = render(&detail(true), "markdown").unwrap();
    assert!(text.contains("中文 \\*标题\\* \\[链接\\]\\(x\\) &lt;tag&gt;"));
    assert!(text.contains("运维\\_一部"));
    assert!(text.contains("首行\r\n    \\# 次行"));
    assert!(text.contains("（无）"));
    let headings = [
        "本班新增",
        "本班已处理",
        "当前待处理",
        "后续跟进",
        "重点关注",
    ]
    .map(|s| text.find(&format!("## {s}")).unwrap());
    assert!(headings.windows(2).all(|w| w[0] < w[1]));
    assert!(!text.replace("\r\n", "").contains(['\r', '\n']));
    assert!(render(&detail(true), "html").is_err());
}

#[test]
fn preview_and_export_share_exact_utf8_bytes_and_confirmed_overwrite() {
    let d = detail(true);
    let dir = dir();
    let path = dir.join("中文交接.md");
    let expected = render(&d, "markdown").unwrap();
    let result = export(&d, "markdown", &path, false).unwrap();
    assert_eq!(result.bytes, expected.len() as u64);
    assert_eq!(std::fs::read(&path).unwrap(), expected.as_bytes());
    let txt = render(&d, "txt").unwrap();
    assert!(txt.contains("中文 *标题* [链接](x) <tag>"));
    let err = export(&d, "txt", &path, false).unwrap_err();
    assert_eq!(err.code(), "conflict");
    assert_eq!(std::fs::read(&path).unwrap(), expected.as_bytes());
    export(&d, "txt", &path, true).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), txt.as_bytes());
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        1,
        "临时文件必须清理"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn draft_and_failed_io_do_not_publish_or_change_target() {
    let dir = dir();
    let path = dir.join("交接.txt");
    assert!(export(&detail(false), "txt", &path, false).is_err());
    assert!(!path.exists());
    let target_directory = dir.join("existing-directory");
    std::fs::create_dir(&target_directory).unwrap();
    let marker = target_directory.join("原文件");
    std::fs::write(&marker, "保留").unwrap();
    assert!(export(&detail(true), "txt", &target_directory, true).is_err());
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "保留");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    assert!(export(
        &detail(true),
        "txt",
        &dir.join("missing-parent").join("x.txt"),
        true
    )
    .is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(windows)]
#[test]
fn locked_windows_destination_preserves_original_file() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = dir();
    let path = dir.join("locked.txt");
    std::fs::write(&path, b"original").unwrap();
    let handle = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    assert!(export(&detail(true), "txt", &path, true).is_err());
    drop(handle);
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn render_orders_sections_and_items_even_if_input_order_changes() {
    let mut d = detail(true);
    let mut first = d.items[0].clone();
    first.id = "a".into();
    first.section = "new".into();
    first.sort_order = 2;
    first.snapshot.task.title = "第一条".into();
    let mut second = first.clone();
    second.id = "b".into();
    second.sort_order = 4;
    second.snapshot.task.title = "第二条".into();
    d.items.insert(0, second);
    d.items.push(first);
    let rendered = render(&d, "txt").unwrap();
    assert!(rendered.find("第一条").unwrap() < rendered.find("第二条").unwrap());
    assert!(rendered.find("第二条").unwrap() < rendered.find("中文 *标题*").unwrap());
}
