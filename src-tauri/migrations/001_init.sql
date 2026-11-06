-- ShiftNote 初始数据库结构（v1）
-- 时间统一使用 Unix 毫秒时间戳（INTEGER，UTC）存储，显示时转换为本地时间。
-- 业务主键统一使用 UUID（TEXT）。

CREATE TABLE IF NOT EXISTS categories (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL,
    color      TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_categories_name ON categories (name);

CREATE TABLE IF NOT EXISTS tasks (
    id           TEXT PRIMARY KEY NOT NULL,
    title        TEXT NOT NULL,
    description  TEXT,
    priority     INTEGER NOT NULL DEFAULT 2 CHECK (priority BETWEEN 0 AND 3),
    status       TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'done')),
    completed_at INTEGER,
    hidden       INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0, 1)),
    archived     INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    deleted_at   INTEGER,
    category_id  TEXT REFERENCES categories (id) ON DELETE SET NULL,
    assignee     TEXT,
    recorder     TEXT,
    handover_to  TEXT,
    due_at       INTEGER,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tasks_priority ON tasks (priority, sort_order);
CREATE INDEX IF NOT EXISTS idx_tasks_created ON tasks (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_tasks_updated ON tasks (updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks (status, hidden, deleted_at);
CREATE INDEX IF NOT EXISTS idx_tasks_category ON tasks (category_id);
CREATE INDEX IF NOT EXISTS idx_tasks_completed ON tasks (completed_at DESC);

-- 事项进展记录：只追加，不覆盖；编辑时保留 edited_at
CREATE TABLE IF NOT EXISTS task_updates (
    id         TEXT PRIMARY KEY NOT NULL,
    task_id    TEXT NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    content    TEXT NOT NULL,
    author     TEXT,
    kind       TEXT NOT NULL DEFAULT 'progress' CHECK (kind IN ('progress', 'system')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    edited_at  INTEGER
);

CREATE INDEX IF NOT EXISTS idx_task_updates_task ON task_updates (task_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_task_updates_created ON task_updates (created_at DESC);

-- 提醒配置与状态（每个事项最多一条）
CREATE TABLE IF NOT EXISTS reminders (
    task_id          TEXT PRIMARY KEY NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    enabled          INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    interval_minutes INTEGER NOT NULL DEFAULT 120,
    first_remind_at  INTEGER,
    next_remind_at   INTEGER,
    last_notified_at INTEGER,
    snooze_until     INTEGER,
    remind_count     INTEGER NOT NULL DEFAULT 0,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reminders_due ON reminders (enabled, next_remind_at);
CREATE INDEX IF NOT EXISTS idx_reminders_snooze ON reminders (snooze_until);

-- 交接班记录
CREATE TABLE IF NOT EXISTS handovers (
    id            TEXT PRIMARY KEY NOT NULL,
    handover_date TEXT NOT NULL,
    shift_from    TEXT,
    shift_to      TEXT,
    note          TEXT,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_handovers_date ON handovers (handover_date DESC, created_at DESC);

-- 交接班与事项的关联：只引用事项，不复制事项数据
CREATE TABLE IF NOT EXISTS handover_tasks (
    handover_id TEXT NOT NULL REFERENCES handovers (id) ON DELETE CASCADE,
    task_id     TEXT NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    section     TEXT NOT NULL DEFAULT 'pending'
                CHECK (section IN ('new', 'handled', 'pending', 'follow_up', 'focus')),
    note        TEXT,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (handover_id, task_id)
);

CREATE INDEX IF NOT EXISTS idx_handover_tasks_task ON handover_tasks (task_id);

-- 用户配置（键值对）
CREATE TABLE IF NOT EXISTS settings (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
