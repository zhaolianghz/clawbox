-- 缺字段的 fixture:验证 hermes adapter 在 schema 版本不同时也能容错。
-- 表里没有 cache_write_tokens 列(模拟旧版 hermes)→ adapter 取 0。

CREATE TABLE session_model_usage (
    session_id TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens INTEGER NOT NULL DEFAULT 0,
    first_seen REAL,
    last_seen REAL,
    PRIMARY KEY (session_id, model)
);

INSERT INTO session_model_usage VALUES
  ('legacy-1', 'MiniMax-M2.7-highspeed', 100, 50, 0, 0, 1775893000.0, 1775893200.0);
