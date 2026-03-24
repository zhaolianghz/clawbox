import { invoke } from '@tauri-apps/api/core';

export interface LogFile {
  name: string;
  path: string;
  size: number;
  modified: string;
}

export interface LogLine {
  timestamp: string;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
}

export async function get_log_files(): Promise<LogFile[]> {
  try {
    return await invoke<LogFile[]>('get_log_files');
  } catch {
    return get_mock_log_files();
  }
}

export async function get_log_content(path: string, filter?: string): Promise<LogLine[]> {
  try {
    return await invoke<LogLine[]>('get_log_content', { path, filter });
  } catch {
    return get_mock_log_content(filter);
  }
}

function get_mock_log_files(): LogFile[] {
  return [
    { name: 'gateway-2024-03-24.log', path: '/logs/gateway-2024-03-24.log', size: 102400, modified: '2024-03-24 15:30:00' },
    { name: 'gateway-2024-03-23.log', path: '/logs/gateway-2024-03-23.log', size: 204800, modified: '2024-03-23 23:59:00' },
    { name: 'gateway-2024-03-22.log', path: '/logs/gateway-2024-03-22.log', size: 153600, modified: '2024-03-22 23:59:00' },
    { name: 'agent-2024-03-24.log', path: '/logs/agent-2024-03-24.log', size: 51200, modified: '2024-03-24 15:30:00' },
    { name: 'error-2024-03-24.log', path: '/logs/error-2024-03-24.log', size: 8192, modified: '2024-03-24 14:20:00' },
  ];
}

function get_mock_log_content(filter?: string): LogLine[] {
  const logs: LogLine[] = [
    { timestamp: '2024-03-24 15:30:00', level: 'info', message: 'Gateway started on port 18789' },
    { timestamp: '2024-03-24 15:30:01', level: 'info', message: 'Loaded 3 agents from config' },
    { timestamp: '2024-03-24 15:30:02', level: 'debug', message: 'Initializing channel: openai' },
    { timestamp: '2024-03-24 15:30:02', level: 'debug', message: 'Initializing channel: anthropic' },
    { timestamp: '2024-03-24 15:30:03', level: 'info', message: 'API server ready' },
    { timestamp: '2024-03-24 15:31:00', level: 'info', message: 'Request: POST /v1/chat/completions from 127.0.0.1' },
    { timestamp: '2024-03-24 15:31:01', level: 'debug', message: 'Routing request to agent: claude-3' },
    { timestamp: '2024-03-24 15:31:05', level: 'info', message: 'Response: 200 OK (1.2s, 245 tokens)' },
    { timestamp: '2024-03-24 15:32:00', level: 'warn', message: 'Rate limit approaching for channel: openai' },
    { timestamp: '2024-03-24 15:33:00', level: 'info', message: 'Request: POST /v1/chat/completions from 127.0.0.1' },
    { timestamp: '2024-03-24 15:33:02', level: 'error', message: 'Failed to connect to upstream: connection timeout' },
    { timestamp: '2024-03-24 15:33:02', level: 'info', message: 'Retrying with fallback channel...' },
    { timestamp: '2024-03-24 15:33:05', level: 'info', message: 'Response: 200 OK via fallback (3.1s, 180 tokens)' },
  ];
  
  if (filter) {
    const lowerFilter = filter.toLowerCase();
    return logs.filter(log => 
      log.message.toLowerCase().includes(lowerFilter) ||
      log.level.toLowerCase().includes(lowerFilter)
    );
  }
  
  return logs;
}
