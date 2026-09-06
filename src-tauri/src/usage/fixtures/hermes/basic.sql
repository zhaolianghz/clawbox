-- Hermes `state.db` fixture:3 sessions × 2-3 model rows。
-- PRIMARY KEY = (session_id, model, billing_provider, billing_base_url, billing_mode, task)。
-- 时间戳:unix epoch seconds(REAL,与 hermes 实际 schema 一致)。

CREATE TABLE session_model_usage (
    session_id TEXT NOT NULL,
    model TEXT NOT NULL,
    billing_provider TEXT NOT NULL DEFAULT '',
    billing_base_url TEXT NOT NULL DEFAULT '',
    billing_mode TEXT NOT NULL DEFAULT '',
    task TEXT NOT NULL DEFAULT '',
    api_call_count INTEGER NOT NULL DEFAULT 0,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens INTEGER NOT NULL DEFAULT 0,
    estimated_cost_usd REAL NOT NULL DEFAULT 0,
    actual_cost_usd REAL NOT NULL DEFAULT 0,
    cost_status TEXT,
    cost_source TEXT,
    first_seen REAL,
    last_seen REAL,
    PRIMARY KEY (session_id, model, billing_provider, billing_base_url, billing_mode, task)
);

INSERT INTO session_model_usage VALUES
  -- session 1: 同一 session, 2 个不同 model 维度
  ('20260411_153639_3381a3', 'MiniMax-M2.7-highspeed', 'minimax-cn', 'https://api.minimaxi.com/anthropic', '', '',
   0,  74, 49,     0, 12770, 0, 0.0, 0.0, 'unknown', 'none', 1775893004.05875, 1775893159.54219),
  ('20260411_153639_3381a3', 'deepseek-v4-flash',    'deepseek',  'https://api.deepseek.com/anthropic', '', '',
   0, 200, 80,     0,     0, 0, 0.0, 0.0, 'unknown', 'none', 1775893100.00000, 1775893200.00000),
  -- session 2: 单 model, 多次调用累加(主键不冲突因为 session 不同)
  ('20260411_154740_037dda', 'MiniMax-M2.7-highspeed', 'minimax-cn', 'https://api.minimaxi.com/anthropic', '', '',
   0, 2847161, 20750, 2680476, 484711, 0, 0.0, 0.0, 'unknown', 'none', 1775893670.14046, 1775904008.07843),
  -- session 3: 跨 task 维度(主键带 task,验证 task 维度拆分)
  ('20260411_154958_ccddc6dc', 'MiniMax-M2.7-highspeed', 'minimax-cn', 'https://api.minimaxi.com/anthropic', '', 'main',
   0,  241,    239, 23754, 6096, 0, 0.0, 0.0, 'unknown', 'none', 1775893798.19417, 1775977926.18195),
  ('20260411_154958_ccddc6dc', 'MiniMax-M2.7-highspeed', 'minimax-cn', 'https://api.minimaxi.com/anthropic', '', 'compact',
   0,   10,      5,    50,   10, 0, 0.0, 0.0, 'unknown', 'none', 1775893800.00000, 1775893850.00000);
