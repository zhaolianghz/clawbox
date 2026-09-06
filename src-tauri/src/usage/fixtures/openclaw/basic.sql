-- openclaw 桌面端 fixture:模拟 runtime SQLite(会话 + transcript 都存在同一 db)。
-- 文档里 schema 是 OpenClaw 1.x 的,字段名按 openclaw doc 命名。
-- 我们只关心 `transcript` 表(消息/usage)。
-- 因为 schema 文档分散,这里用保守字段名,fixture 简化为单一 usage 表。
-- 实际生产 schema 可能含更多列,我们只在 SELECT 时用 COALESCE 兜底。

CREATE TABLE transcript (
    id varchar(64) primary key,
    session_id varchar(64),
    role varchar(32),
    model varchar(128) DEFAULT '',
    input_tokens INTEGER DEFAULT 0,
    output_tokens INTEGER DEFAULT 0,
    cache_read_tokens INTEGER DEFAULT 0,
    cache_write_tokens INTEGER DEFAULT 0,
    created_at INTEGER DEFAULT 0
);

INSERT INTO transcript VALUES
  ('t1', 'sess-1', 'user',      '',             0,    0,    0,    0, 1787000000000),
  ('t2', 'sess-1', 'assistant', 'claude-opus-5', 1500, 400,  200, 100, 1787000010000),
  ('t3', 'sess-1', 'assistant', 'claude-opus-5', 800,  150,  0,   0,   1787000020000),
  ('t4', 'sess-2', 'assistant', 'gpt-5.4',       1200, 300,  0,   0,   1787000030000),
  -- 没有 token(role=user / 空 model)
  ('t5', 'sess-2', 'user',      '',             0,    0,    0,    0,  1787000040000);
