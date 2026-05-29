import { APP_CONFIG } from '$lib/config';
import { invoke } from '@tauri-apps/api/core';

// Gateway WebSocket types
interface WsMessage {
  id?: string;
  method?: string;
  params?: Record<string, unknown>;
  result?: unknown;
  error?: unknown;
  nonce?: string;
}

interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: Date;
}

// Get gateway auth token - first from localStorage, or fetch from Tauri backend
async function getGatewayToken(): Promise<string> {
  const stored = localStorage.getItem('openclaw_gateway_token');
  if (stored) return stored;

  try {
    const token = await invoke<string>('get_gateway_token');
    localStorage.setItem('openclaw_gateway_token', token);
    return token;
  } catch (err) {
    console.error('Failed to get gateway token:', err);
    return '';
  }
}

class OpenClawGateway {
  private ws: WebSocket | null = null;
  private messageId = 0;
  private pendingRequests = new Map<string, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000;
  private onMessageCallback: ((msg: ChatMessage) => void) | null = null;
  private onStatusChangeCallback: ((status: 'connected' | 'disconnected' | 'connecting') => void) | null = null;
  private sessionKey: string | null = null;

  constructor() {
    // Check for stored session
    this.sessionKey = localStorage.getItem('chat_session_key');
  }

  async connect(onMessage?: (msg: ChatMessage) => void, onStatusChange?: (status: 'connected' | 'disconnected' | 'connecting') => void): Promise<void> {
    this.onMessageCallback = onMessage || null;
    this.onStatusChangeCallback = onStatusChange || null;

    return new Promise(async (resolve, reject) => {
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const host = APP_CONFIG.gateway.host;
      const port = APP_CONFIG.gateway.port;
      const token = await getGatewayToken();

      // Token can be passed as query param for WebSocket auth
      const wsUrl = token
        ? `ws://${host}:${port}?token=${encodeURIComponent(token)}`
        : `ws://${host}:${port}`;

      try {
        this.ws = new WebSocket(wsUrl);
      } catch (err) {
        reject(new Error(`Failed to create WebSocket: ${err}`));
        return;
      }

      this.ws.onopen = () => {
        this.reconnectAttempts = 0;
        onStatusChange?.('connected');
        resolve();
      };

      this.ws.onmessage = (event) => {
        try {
          const msg: WsMessage = JSON.parse(event.data);
          this.handleMessage(msg);
        } catch {
          console.error('Failed to parse WebSocket message');
        }
      };

      this.ws.onerror = (error) => {
        console.error('WebSocket error:', error);
      };

      this.ws.onclose = (event) => {
        onStatusChange?.('disconnected');
        if (this.reconnectAttempts < this.maxReconnectAttempts) {
          this.reconnectAttempts++;
          setTimeout(() => {
            this.connect(this.onMessageCallback || undefined, this.onStatusChangeCallback || undefined);
          }, this.reconnectDelay * this.reconnectAttempts);
        }
      };
    });
  }

  private handleMessage(msg: WsMessage): void {
    if (msg.id && this.pendingRequests.has(msg.id)) {
      const { resolve, reject } = this.pendingRequests.get(msg.id)!;
      this.pendingRequests.delete(msg.id);
      if (msg.error) {
        reject(new Error(JSON.stringify(msg.error)));
      } else {
        resolve(msg.result);
      }
    }

    // Handle incoming chat messages (from agent responses)
    if (msg.method === 'chat.pending' || msg.method === 'chat.delta') {
      // These are streaming events - could be handled for real-time display
    }

    if (msg.method === 'chat.complete') {
      // Chat completed - could update UI
    }
  }

  private send(method: string, params: Record<string, unknown> = {}): Promise<unknown> {
    return new Promise((resolve, reject) => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        reject(new Error('WebSocket not connected'));
        return;
      }

      const id = `msg_${++this.messageId}`;
      const payload: WsMessage = { id, method, params };

      this.pendingRequests.set(id, { resolve, reject });
      this.ws.send(JSON.stringify(payload));

      // Timeout after 60 seconds
      setTimeout(() => {
        if (this.pendingRequests.has(id)) {
          this.pendingRequests.delete(id);
          reject(new Error(`Request ${method} timed out`));
        }
      }, 60000);
    });
  }

  async getHistory(): Promise<ChatMessage[]> {
    try {
      const result = await this.send('chat.history', {
        maxEntries: 50,
        truncateOversized: true,
      }) as { entries: Array<{ id: string; role: string; content: string; timestamp: string }> };

      if (!result?.entries) return [];

      return result.entries.map((entry) => ({
        id: entry.id,
        role: entry.role as 'user' | 'assistant',
        content: entry.content,
        timestamp: new Date(entry.timestamp),
      }));
    } catch (err) {
      console.error('Failed to get chat history:', err);
      return [];
    }
  }

  async sendMessage(content: string): Promise<ChatMessage> {
    const result = await this.send('chat.send', {
      text: content,
      // If we have an existing session key, use it
      ...(this.sessionKey ? { sessionKey: this.sessionKey } : {}),
    }) as { sessionKey?: string; entry?: { id: string; role: string; content: string; timestamp: string } };

    if (result?.sessionKey) {
      this.sessionKey = result.sessionKey;
      localStorage.setItem('chat_session_key', result.sessionKey);
    }

    if (result?.entry) {
      return {
        id: result.entry.id,
        role: result.entry.role as 'user' | 'assistant',
        content: result.entry.content,
        timestamp: new Date(result.entry.timestamp),
      };
    }

    throw new Error('No response entry in result');
  }

  async injectNote(content: string): Promise<void> {
    await this.send('chat.inject', { text: content });
  }

  disconnect(): void {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    this.pendingRequests.clear();
  }

  isConnected(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }
}

// Singleton instance
export const openclawGateway = new OpenClawGateway();

// Export types
export type { ChatMessage };
