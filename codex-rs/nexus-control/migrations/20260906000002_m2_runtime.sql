-- Nexus M2: 执行闭环增量 schema。
-- items 加 codex_item_id（app-server ThreadItem.id 字符串）做幂等键：
--   item/started 与 item/completed 同 id → ON CONFLICT(codex_item_id) DO UPDATE。
-- app_server_events 已有 UNIQUE(thread_id,seq)，做原始事件流幂等。
-- turns 加 model 列已在 M1（model TEXT）—— M2 填充。

ALTER TABLE items ADD COLUMN IF NOT EXISTS codex_item_id TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_items_codex_item_id ON items(codex_item_id) WHERE codex_item_id IS NOT NULL;

-- turn interrupt/failed 状态值落 status；M1 默认 'pending'/'running'/'completed'。
