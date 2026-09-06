-- opencode `opencode.db` fixture:简化版 schema,
-- 只保留 message 关键列。token 字段在 data JSON 内。

CREATE TABLE `session` (
    `id` text PRIMARY KEY
);

CREATE TABLE `message` (
    `id` text PRIMARY KEY,
    `session_id` text NOT NULL,
    `time_created` integer NOT NULL,
    `time_updated` integer NOT NULL,
    `data` text NOT NULL
);

INSERT INTO `session` VALUES ('sess-001'), ('sess-002');

-- time_created 是 unix epoch ms
INSERT INTO `message` VALUES
  ('msg-1', 'sess-001', 1787934051917, 1787934059294,
   '{"role":"assistant","mode":"build","agent":"build","path":{"cwd":"/Users/test/proj1"},"cost":0,"tokens":{"total":43350,"input":42228,"output":98,"reasoning":0,"cache":{"write":0,"read":1024}},"modelID":"mimo-v2.5-free","providerID":"opencode","time":{"created":1787934051917,"completed":1787934059294},"finish":"stop"}'),
  ('msg-2', 'sess-001', 1787934100000, 1787934105000,
   '{"role":"assistant","mode":"build","agent":"build","cost":0,"tokens":{"total":1500,"input":800,"output":200,"reasoning":50,"cache":{"write":0,"read":200}},"modelID":"mimo-v2.5-free","providerID":"opencode","time":{"created":1787934100000,"completed":1787934105000},"finish":"stop"}'),
  ('msg-3', 'sess-002', 1787934200000, 1787934204000,
   '{"role":"assistant","mode":"plan","agent":"plan","cost":0,"tokens":{"total":2000,"input":1800,"output":200,"reasoning":0,"cache":{"write":500,"read":0}},"modelID":"claude-sonnet-4-5","providerID":"anthropic","time":{"created":1787934200000,"completed":1787934204000},"finish":"stop"}'),
  -- 用户消息 → role != assistant → 必须跳过
  ('msg-4', 'sess-001', 1787934050000, 1787934050000,
   '{"role":"user","content":"hi"}'),
  -- assistant 但没 tokens 字段 → 跳过(只计数 lines_total)
  ('msg-5', 'sess-001', 1787934060000, 1787934060000,
   '{"role":"assistant","modelID":"mimo-v2.5-free","finish":"tool_calls"}');
