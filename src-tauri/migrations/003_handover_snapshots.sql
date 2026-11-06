-- v3：草稿引用实时事项，定稿以独立 JSON 保存当时内容。
-- 不改写 v1/v2；已有交接关联保留为草稿，按旧日期回填 UTC 日区间。
ALTER TABLE handovers ADD COLUMN title TEXT NOT NULL DEFAULT '';
ALTER TABLE handovers ADD COLUMN period_start INTEGER NOT NULL DEFAULT 0;
ALTER TABLE handovers ADD COLUMN period_end INTEGER NOT NULL DEFAULT 1;
ALTER TABLE handovers ADD COLUMN status TEXT NOT NULL DEFAULT 'draft'
    CHECK (status IN ('draft', 'finalized'));
ALTER TABLE handovers ADD COLUMN finalized_at INTEGER;
ALTER TABLE handovers ADD COLUMN snapshot_version INTEGER NOT NULL DEFAULT 1;

UPDATE handovers
   SET title = handover_date || ' 交接记录',
       period_start = COALESCE(CAST(strftime('%s', handover_date || ' 00:00:00') AS INTEGER) * 1000, created_at),
       period_end = COALESCE(CAST(strftime('%s', handover_date || ' 00:00:00') AS INTEGER) * 1000, created_at) + 86400000;

CREATE TABLE handover_tasks_v3 (
    id                       TEXT PRIMARY KEY NOT NULL,
    handover_id              TEXT NOT NULL REFERENCES handovers (id) ON DELETE CASCADE,
    task_id                  TEXT REFERENCES tasks (id) ON DELETE SET NULL,
    section                  TEXT NOT NULL DEFAULT 'pending'
        CHECK (section IN ('new', 'handled', 'pending', 'follow_up', 'focus')),
    note                     TEXT,
    sort_order               INTEGER NOT NULL DEFAULT 0,
    selected_update_ids_json TEXT NOT NULL DEFAULT '[]',
    snapshot_json            TEXT,
    UNIQUE (handover_id, task_id)
);

INSERT INTO handover_tasks_v3 (id, handover_id, task_id, section, note, sort_order)
SELECT lower(hex(randomblob(16))), handover_id, task_id, section, note, sort_order
  FROM handover_tasks;
DROP TABLE handover_tasks;
ALTER TABLE handover_tasks_v3 RENAME TO handover_tasks;
CREATE INDEX idx_handover_tasks_task ON handover_tasks (task_id);
CREATE INDEX idx_handover_tasks_order ON handover_tasks (handover_id, sort_order, id);
CREATE INDEX idx_handovers_status ON handovers (status, period_start DESC, id);
