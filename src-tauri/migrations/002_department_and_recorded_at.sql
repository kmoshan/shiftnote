-- 002：字段收敛
--
-- 1) 把「记录人 / 负责人 / 交接对象」三个自由文本合并为单一「负责部门」；
--    旧数据按 assignee → recorder → handover_to 的优先级回填，不丢信息。
-- 2) 用「记录时间」取代「预计完成时间」：新增 recorded_at，
--    历史数据以 created_at 回填，保证每条事项都有记录时间。
--
-- 旧的 assignee / recorder / handover_to / due_at 列保留在表中（不再读写），
-- 便于用户回退旧版本或在需要时手工取回数据。

ALTER TABLE tasks ADD COLUMN department TEXT;
ALTER TABLE tasks ADD COLUMN recorded_at INTEGER;

UPDATE tasks SET recorded_at = created_at WHERE recorded_at IS NULL;

UPDATE tasks
   SET department = COALESCE(
        NULLIF(TRIM(assignee), ''),
        NULLIF(TRIM(recorder), ''),
        NULLIF(TRIM(handover_to), '')
   )
 WHERE department IS NULL;

CREATE INDEX IF NOT EXISTS idx_tasks_recorded ON tasks (recorded_at DESC);
