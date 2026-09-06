-- Qoder 桌面端 (macOS) `local.db` fixture:
-- 表 chat_session / chat_record / chat_message,token 数字在 chat_message.token_info
-- (TEXT,JSON),`extra` 里有 modelConfig.key。本 fixture 模拟 3 个 session × 2 个 model 的混合用法。

CREATE TABLE chat_session (
    session_id varchar(64) primary key,
    user_name varchar(64),
    session_title varchar(256),
    gmt_create INTEGER,
    gmt_modified INTEGER,
    preferred_model_info TEXT DEFAULT ''
);

CREATE TABLE chat_record (
    request_id varchar(64) primary key,
    session_id varchar(64) not null,
    question text,
    answer text,
    gmt_create INTEGER,
    gmt_modified INTEGER,
    extra text DEFAULT '{}',
    mode varchar(64) DEFAULT ''
);

CREATE TABLE chat_message (
    id varchar(64) primary key,
    session_id varchar(64),
    request_id varchar(64),
    role varchar(64),
    content text,
    gmt_create INTEGER,
    token_info text DEFAULT '',
    model_info text DEFAULT ''
);

INSERT INTO chat_session VALUES
  ('sess-A', 'skyzhao', '实现用量统计', 1765545980000, 1765546100000, '{"key":"auto","name":"Auto"}'),
  ('sess-B', 'skyzhao', '修复 bug', 1765547000000, 1765547100000, '{"key":"claude-sonnet-4-5"}');

-- session A 两条 assistant message,model 都用 chat_record.extra 里的 modelConfig
INSERT INTO chat_record VALUES
  ('req-A1', 'sess-A', 'hi', 'reply-1', 1765545985506, 1765545985506,
   '{"context":null,"modelConfig":{"key":"claude-opus-4-5","name":"Claude Opus 4.5"}}', 'common_agent'),
  ('req-A2', 'sess-A', 'another', 'reply-2', 1765545990000, 1765545990000,
   '{"context":null,"modelConfig":{"key":"claude-opus-4-5","name":"Claude Opus 4.5"}}', 'common_agent'),
  ('req-B1', 'sess-B', '?', '!', 1765547000000, 1765547000000,
   '{"context":null,"modelConfig":{"key":"claude-sonnet-4-5","name":"Claude Sonnet 4.5"}}', 'common_agent');

INSERT INTO chat_message VALUES
  -- req-A1
  ('m-A1u', 'sess-A', 'req-A1', 'user',     'hi',     1765545985500, '',                 ''),
  ('m-A1a', 'sess-A', 'req-A1', 'assistant', 'reply-1', 1765545985581,
   '{"prompt_tokens":14893,"completion_tokens":267,"cached_tokens":0,"max_input_tokens":180000}',
   '{"model_key":"claude-opus-4-5"}'),
  -- req-A2
  ('m-A2u', 'sess-A', 'req-A2', 'user',     'another', 1765545990000, '',                ''),
  ('m-A2a', 'sess-A', 'req-A2', 'assistant', 'reply-2', 1765545990200,
   '{"prompt_tokens":36640,"completion_tokens":199,"cached_tokens":14891,"max_input_tokens":180000}',
   '{"model_key":"claude-opus-4-5"}'),
  -- req-B1
  ('m-B1u', 'sess-B', 'req-B1', 'user',     '?',       1765547000000, '',                ''),
  ('m-B1a', 'sess-B', 'req-B1', 'assistant', '!',      1765547000300,
   '{"prompt_tokens":200,"completion_tokens":50,"cached_tokens":0,"max_input_tokens":180000}',
   '{"model_key":"claude-sonnet-4-5"}'),
  -- 没有 token_info 的 assistant message → 应跳过
  ('m-A1a-empty', 'sess-A', 'req-A1', 'assistant', 'old-broken', 1765545985400, '', '');
