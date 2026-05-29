// Centralized app configuration
// In Tauri, these come from environment variables set in tauri.conf.json
// or from the .env file

const GATEWAY_PORT = 18789;
const GATEWAY_HOST = 'localhost';
const DESKTOP_PORT = 18791;
const OPENCLAW_COMMAND = 'openclaw';
const OPENCLAW_GATEWAY_COMMAND = 'openclaw-gateway';

export const APP_CONFIG = {
  gateway: {
    port: GATEWAY_PORT,
    host: GATEWAY_HOST,
  },
  desktop: {
    port: DESKTOP_PORT,
  },
  openclaw: {
    command: OPENCLAW_COMMAND,
    gatewayCommand: OPENCLAW_GATEWAY_COMMAND,
  },
} as const;

export type AppConfig = typeof APP_CONFIG;

// For dynamic updates at runtime
export function updateGatewayPort(port: number): void {
  (APP_CONFIG.gateway as { port: number }).port = port;
}
