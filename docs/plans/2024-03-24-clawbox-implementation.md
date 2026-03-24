# ClawBox Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build ClawBox - a cross-platform desktop GUI client for OpenClaw Gateway using Tauri v2 + Svelte + Rust

**Architecture:** 
- Rust backend via Tauri v2 handles system operations, CLI calls, file I/O
- Svelte frontend handles UI with dark neon gaming theme
- Tauri IPC bridges frontend and backend
- Multi-language support via svelte-i18n and rust-i18n

**Tech Stack:**
- Backend: Tauri v2, Rust
- Frontend: SvelteKit, Svelte 5, TailwindCSS 4, Skeleton UI 3
- State: Svelte runes ($state, $derived)
- i18n: svelte-i18n, rust-i18n

---

## Phase 1: Project Setup & Core Infrastructure

### Task 1: Initialize Tauri v2 Project

**Files:**
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`
- Create: `package.json`
- Create: `svelte.config.js`
- Create: `vite.config.ts`
- Create: `tailwind.config.js`

**Step 1: Create Tauri project**

```bash
cd /Users/skyzhao/code/sky-project/clawbox
npm create tauri-app@latest . -- --template svelte-ts
```

**Step 2: Install dependencies**

```bash
npm install
npm install -D tailwindcss postcss autoprefixer @tailwindcss/vite
npm install @skeletonlabs/skeleton-svelte @skeletonlabs/skeleton
npx tailwindcss init -p
```

**Step 3: Configure TailwindCSS**

Create `tailwind.config.js`:
```javascript
/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./src/**/*.{js,ts,svelte}",
    "./index.html",
  ],
  theme: {
    extend: {
      colors: {
        neon: {
          cyan: '#00f5ff',
          purple: '#bf00ff',
          pink: '#ff006e',
          green: '#00ff88',
          orange: '#ff8800',
        }
      }
    },
  },
  plugins: [],
}
```

**Step 4: Verify build**

Run: `npm run tauri dev`
Expected: Window opens with default Svelte app

**Commit:** `git add -A && git commit -m "feat: initialize Tauri v2 project with Svelte"`

---

### Task 2: Setup Dark Neon Theme

**Files:**
- Create: `src/app.css`
- Create: `src/lib/styles/neon.css`

**Step 1: Create neon theme CSS**

Create `src/lib/styles/neon.css`:
```css
:root {
  --bg-primary: #0a0a0f;
  --bg-secondary: #12121a;
  --bg-tertiary: #1a1a25;
  
  --neon-cyan: #00f5ff;
  --neon-purple: #bf00ff;
  --neon-pink: #ff006e;
  --neon-green: #00ff88;
  --neon-orange: #ff8800;
  
  --text-primary: #ffffff;
  --text-secondary: #a0a0b0;
  --text-muted: #606070;
  
  --glow-cyan: 0 0 20px rgba(0, 245, 255, 0.5);
  --glow-purple: 0 0 20px rgba(191, 0, 255, 0.5);
  --glow-green: 0 0 15px rgba(0, 255, 136, 0.4);
}

body {
  background-color: var(--bg-primary);
  color: var(--text-primary);
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

.neon-border {
  border: 1px solid var(--neon-cyan);
  box-shadow: var(--glow-cyan);
}

.neon-button {
  background: var(--bg-tertiary);
  border: 1px solid var(--neon-cyan);
  color: var(--neon-cyan);
  padding: 0.5rem 1rem;
  border-radius: 0.5rem;
  transition: all 0.3s ease;
}

.neon-button:hover {
  box-shadow: var(--glow-cyan);
  background: rgba(0, 245, 255, 0.1);
}

.glass-card {
  background: rgba(26, 26, 37, 0.8);
  backdrop-filter: blur(10px);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 1rem;
}
```

**Step 2: Import in app.css**

```css
@import 'tailwindcss';
@import './lib/styles/neon.css';
```

**Step 3: Verify theme**

Run: `npm run tauri dev`
Expected: Dark background with neon accent colors

**Commit:** `git add -A && git commit -m "feat: add dark neon gaming theme"`

---

### Task 3: Setup i18n (Multi-language)

**Files:**
- Create: `src/lib/i18n/en.json`
- Create: `src/lib/i18n/zh.json`
- Create: `src/lib/i18n/index.ts`
- Modify: `src/app.html`

**Step 1: Install svelte-i18n**

```bash
npm install svelte-i18n
```

**Step 2: Create language files**

Create `src/lib/i18n/en.json`:
```json
{
  "app": {
    "name": "ClawBox",
    "welcome": "Welcome back"
  },
  "nav": {
    "home": "Home",
    "chat": "Chat",
    "config": "Config",
    "agents": "Agents",
    "monitor": "Monitor",
    "tasks": "Tasks",
    "logs": "Logs",
    "skills": "Skills",
    "about": "About"
  },
  "gateway": {
    "status": "Status",
    "running": "Running",
    "stopped": "Stopped",
    "start": "Start",
    "stop": "Stop",
    "restart": "Restart"
  }
}
```

Create `src/lib/i18n/zh.json`:
```json
{
  "app": {
    "name": "ClawBox",
    "welcome": "欢迎回来"
  },
  "nav": {
    "home": "首页",
    "chat": "聊天",
    "config": "配置",
    "agents": "代理",
    "monitor": "监控",
    "tasks": "任务",
    "logs": "日志",
    "skills": "技能",
    "about": "关于"
  },
  "gateway": {
    "status": "状态",
    "running": "运行中",
    "stopped": "已停止",
    "start": "启动",
    "stop": "停止",
    "restart": "重启"
  }
}
```

**Step 3: Setup i18n module**

Create `src/lib/i18n/index.ts`:
```typescript
import { init, register } from 'svelte-i18n';

register('en', () => import('./en.json'));
register('zh', () => import('./zh.json'));

init({
  fallbackLocale: 'en',
  initialLocale: navigator.language.startsWith('zh') ? 'zh' : 'en',
});
```

**Step 4: Import in app.html**

Add before closing `</head>`:
```html
<script type="module">
  import './lib/i18n/index.ts';
</script>
```

**Commit:** `git add -A && git commit -m "feat: setup i18n with en/zh support"`

---

## Phase 2: Layout & Navigation

### Task 4: Create Main Layout with Sidebar

**Files:**
- Create: `src/routes/+layout.svelte`
- Create: `src/lib/components/Sidebar.svelte`
- Create: `src/lib/components/TopBar.svelte`
- Create: `src/lib/components/StatusBar.svelte`

**Step 1: Create Sidebar component**

Create `src/lib/components/Sidebar.svelte`:
```svelte
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { _ } from 'svelte-i18n';
  
  interface Props {
    activeItem: string;
  }
  
  let { activeItem }: Props = $props();
  const dispatch = createEventDispatcher();
  
  const navItems = [
    { id: 'home', icon: '🏠', label: 'nav.home' },
    { id: 'chat', icon: '💬', label: 'nav.chat' },
    { id: 'config', icon: '⚙️', label: 'nav.config' },
    { id: 'agents', icon: '🤖', label: 'nav.agents' },
    { id: 'monitor', icon: '📊', label: 'nav.monitor' },
    { id: 'tasks', icon: '📋', label: 'nav.tasks' },
    { id: 'logs', icon: '📝', label: 'nav.logs' },
    { id: 'skills', icon: '🧩', label: 'nav.skills' },
  ];
  
  const bottomItems = [
    { id: 'about', icon: 'ℹ️', label: 'nav.about' },
  ];
  
  function navigate(id: string) {
    dispatch('navigate', id);
  }
</script>

<aside class="sidebar">
  <nav class="nav-main">
    {#each navItems as item}
      <button
        class="nav-item"
        class:active={activeItem === item.id}
        onclick={() => navigate(item.id)}
      >
        <span class="icon">{item.icon}</span>
        <span class="label">{$_(item.label)}</span>
      </button>
    {/each}
  </nav>
  
  <nav class="nav-bottom">
    {#each bottomItems as item}
      <button
        class="nav-item"
        class:active={activeItem === item.id}
        onclick={() => navigate(item.id)}
      >
        <span class="icon">{item.icon}</span>
        <span class="label">{$_(item.label)}</span>
      </button>
    {/each}
  </nav>
</aside>

<style>
  .sidebar {
    width: 200px;
    background: var(--bg-secondary);
    display: flex;
    flex-direction: column;
    border-right: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .nav-main {
    flex: 1;
    padding: 1rem 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  
  .nav-bottom {
    padding: 1rem 0.5rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .nav-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    transition: all 0.2s ease;
    width: 100%;
    text-align: left;
  }
  
  .nav-item:hover {
    background: rgba(0, 245, 255, 0.1);
    color: var(--neon-cyan);
  }
  
  .nav-item.active {
    background: rgba(0, 245, 255, 0.15);
    color: var(--neon-cyan);
    border-left: 3px solid var(--neon-cyan);
    box-shadow: var(--glow-cyan);
  }
  
  .icon {
    font-size: 1.25rem;
  }
</style>
```

**Step 2: Create TopBar component**

Create `src/lib/components/TopBar.svelte`:
```svelte
<script lang="ts">
  import { _ } from 'svelte-i18n';
</script>

<header class="topbar">
  <div class="logo">
    <span class="logo-icon">🎮</span>
    <span class="logo-text">{$_('app.name')}</span>
  </div>
  <div class="actions">
    <button class="icon-btn">🔔</button>
    <button class="icon-btn">⚙️</button>
    <button class="icon-btn avatar">👤</button>
  </div>
</header>

<style>
  .topbar {
    height: 60px;
    background: var(--bg-secondary);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 1.5rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .logo {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  
  .logo-icon {
    font-size: 1.5rem;
  }
  
  .logo-text {
    font-size: 1.25rem;
    font-weight: 700;
    color: var(--neon-cyan);
    text-shadow: var(--glow-cyan);
  }
  
  .actions {
    display: flex;
    gap: 0.5rem;
  }
  
  .icon-btn {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: var(--bg-tertiary);
    border: none;
    font-size: 1.25rem;
    cursor: pointer;
    transition: all 0.2s ease;
  }
  
  .icon-btn:hover {
    box-shadow: var(--glow-cyan);
  }
</style>
```

**Step 3: Create StatusBar component**

Create `src/lib/components/StatusBar.svelte`:
```svelte
<script lang="ts">
  import { _ } from 'svelte-i18n';
  
  interface Props {
    gatewayStatus: string;
    gatewayVersion: string;
    tokenCount: number;
  }
  
  let { gatewayStatus, gatewayVersion, tokenCount }: Props = $props();
</script>

<footer class="statusbar">
  <div class="status-item">
    <span class="status-label">Gateway:</span>
    <span class="status-dot" class:running={gatewayStatus === 'running'}></span>
    <span class="status-value">{$_(`gateway.${gatewayStatus}`)}</span>
    <span class="status-version">{gatewayVersion}</span>
  </div>
  <div class="status-item">
    <span class="status-label">Token:</span>
    <span class="status-value">{tokenCount.toLocaleString()}</span>
  </div>
</footer>

<style>
  .statusbar {
    height: 32px;
    background: var(--bg-secondary);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 1.5rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
    font-size: 0.875rem;
  }
  
  .status-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  
  .status-label {
    color: var(--text-muted);
  }
  
  .status-value {
    color: var(--text-secondary);
  }
  
  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--neon-pink);
  }
  
  .status-dot.running {
    background: var(--neon-green);
    box-shadow: var(--glow-green);
    animation: pulse 2s infinite;
  }
  
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }
</style>
```

**Step 4: Create main layout**

Create `src/routes/+layout.svelte`:
```svelte
<script lang="ts">
  import { goto } from '$app/navigation';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import TopBar from '$lib/components/TopBar.svelte';
  import StatusBar from '$lib/components/StatusBar.svelte';
  
  let activeItem = $state('home');
  let gatewayStatus = $state('stopped');
  let gatewayVersion = $state('v1.0.0');
  let tokenCount = $state(0);
  
  function handleNavigate(event: CustomEvent<string>) {
    activeItem = event.detail;
    goto(`/${event.detail === 'home' ? '' : event.detail}`);
  }
</script>

<div class="app-container">
  <Sidebar {activeItem} onnavigate={handleNavigate} />
  <div class="main-area">
    <TopBar />
    <main class="content">
      <slot />
    </main>
    <StatusBar {gatewayStatus} {gatewayVersion} {tokenCount} />
  </div>
</div>

<style>
  .app-container {
    display: flex;
    height: 100vh;
    overflow: hidden;
  }
  
  .main-area {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  
  .content {
    flex: 1;
    overflow: auto;
    padding: 1.5rem;
  }
</style>
```

**Step 5: Verify layout**

Run: `npm run tauri dev`
Expected: Sidebar + TopBar + Content Area + StatusBar visible

**Commit:** `git add -A && git commit -m "feat: create main layout with sidebar navigation"`

---

## Phase 3: Backend Commands

### Task 5: Setup Rust Backend Structure

**Files:**
- Create: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/commands/gateway.rs`
- Create: `src-tauri/src/commands/config.rs`
- Create: `src-tauri/src/commands/install.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Create commands module**

Create `src-tauri/src/commands/mod.rs`:
```rust
pub mod gateway;
pub mod config;
pub mod install;
```

**Step 2: Create gateway command**

Create `src-tauri/src/commands/gateway.rs`:
```rust
use std::process::Command;

#[derive(serde::Serialize)]
pub struct GatewayStatus {
    status: String,
    version: String,
    pid: Option<i32>,
}

#[tauri::command]
pub fn get_gateway_status() -> GatewayStatus {
    // Check if openclaw process is running
    let output = Command::new("pgrep")
        .arg("-f")
        .arg("openclaw gateway")
        .output();
    
    let (status, pid) = match output {
        Ok(output) if output.status.success() => {
            let pid_str = String::from_utf8_lossy(&output.stdout);
            let pid = pid_str.trim().parse::<i32>().ok();
            ("running".to_string(), pid)
        }
        _ => ("stopped".to_string(), None),
    };
    
    // Get version
    let version = Command::new("openclaw")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    
    GatewayStatus { status, version, pid }
}

#[tauri::command]
pub fn start_gateway() -> Result<(), String> {
    Command::new("openclaw")
        .arg("gateway")
        .arg("start")
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn stop_gateway() -> Result<(), String> {
    Command::new("openclaw")
        .arg("gateway")
        .arg("stop")
        .status()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
```

**Step 3: Create config command**

Create `src-tauri/src/commands/config.rs`:
```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub models: HashMap<String, serde_json::Value>,
    pub channels: HashMap<String, serde_json::Value>,
    pub agents: HashMap<String, serde_json::Value>,
    pub skills: HashMap<String, serde_json::Value>,
}

fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap()
        .join(".clawbox")
        .join("config.json")
}

#[tauri::command]
pub fn get_config() -> Config {
    let path = config_path();
    if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        Config::default()
    }
}

#[tauri::command]
pub fn set_config(path: String, value: serde_json::Value) -> Result<(), String> {
    let mut config = get_config();
    // Parse path like "models.openai.apiKey" and set value
    let parts: Vec<&str> = path.split('.').collect();
    // Implementation details...
    let config_str = serde_json::to_string_pretty(&config)
        .map_err(|e| e.to_string())?;
    
    let config_dir = config_path().parent().unwrap();
    fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
    fs::write(config_path(), config_str).map_err(|e| e.to_string())
}

impl Default for Config {
    fn default() -> Self {
        Config {
            models: HashMap::new(),
            channels: HashMap::new(),
            agents: HashMap::new(),
            skills: HashMap::new(),
        }
    }
}
```

**Step 4: Create install command**

Create `src-tauri/src/commands/install.rs`:
```rust
use std::process::Command;

#[derive(serde::Serialize)]
pub struct SystemCheck {
    nodejs: ComponentStatus,
    openclaw: ComponentStatus,
    platform: String,
    is_china: bool,
}

#[derive(serde::Serialize)]
pub struct ComponentStatus {
    installed: bool,
    version: Option<String>,
}

#[tauri::command]
pub fn check_system() -> SystemCheck {
    let nodejs = check_nodejs();
    let openclaw = check_openclaw();
    let platform = std::env::consts::OS.to_string();
    let is_china = check_china_network();
    
    SystemCheck { nodejs, openclaw, platform, is_china }
}

fn check_nodejs() -> ComponentStatus {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| ComponentStatus {
            installed: true,
            version: Some(String::from_utf8_lossy(&o.stdout).trim().to_string()),
        })
        .unwrap_or(ComponentStatus {
            installed: false,
            version: None,
        })
}

fn check_openclaw() -> ComponentStatus {
    Command::new("openclaw")
        .arg("--version")
        .output()
        .map(|o| ComponentStatus {
            installed: true,
            version: Some(String::from_utf8_lossy(&o.stdout).trim().to_string()),
        })
        .unwrap_or(ComponentStatus {
            installed: false,
            version: None,
        })
}

fn check_china_network() -> bool {
    // Try to access a China-specific URL
    Command::new("curl")
        .arg("-s")
        .arg("--connect-timeout")
        .arg("2")
        .arg("https://registry.npmmirror.com")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tauri::command]
pub async fn install_openclaw(use_mirror: bool) -> Result<String, String> {
    let install_script = if use_mirror {
        "npm install -g openclaw --registry=https://registry.npmmirror.com"
    } else {
        "npm install -g openclaw"
    };
    
    let output = Command::new("sh")
        .arg("-c")
        .arg(install_script)
        .output()
        .map_err(|e| e.to_string())?;
    
    if output.status.success() {
        Ok("Installation complete".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
```

**Step 5: Register commands in lib.rs**

Modify `src-tauri/src/lib.rs`:
```rust
mod commands;

use commands::{gateway, config, install};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            gateway::get_gateway_status,
            gateway::start_gateway,
            gateway::stop_gateway,
            config::get_config,
            config::set_config,
            install::check_system,
            install::install_openclaw,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 6: Add dependencies to Cargo.toml**

Add to `src-tauri/Cargo.toml`:
```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dirs = "5"
tauri-plugin-shell = "2"
```

**Step 7: Verify commands**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: Compiles without errors

**Commit:** `git add -A && git commit -m "feat: add Rust backend commands for gateway, config, install"`

---

## Phase 4: Frontend Pages

### Task 6: Create Home/Dashboard Page

**Files:**
- Create: `src/routes/+page.svelte`
- Create: `src/lib/components/Dashboard/GatewayCard.svelte`
- Create: `src/lib/components/Dashboard/StatsCard.svelte`
- Create: `src/lib/api/gateway.ts`

**Step 1: Create API wrapper**

Create `src/lib/api/gateway.ts`:
```typescript
import { invoke } from '@tauri-apps/api/core';

export interface GatewayStatus {
  status: string;
  version: string;
  pid: number | null;
}

export async function getGatewayStatus(): Promise<GatewayStatus> {
  return invoke('get_gateway_status');
}

export async function startGateway(): Promise<void> {
  return invoke('start_gateway');
}

export async function stopGateway(): Promise<void> {
  return invoke('stop_gateway');
}
```

**Step 2: Create GatewayCard component**

Create `src/lib/components/Dashboard/GatewayCard.svelte`:
```svelte
<script lang="ts">
  import { _ } from 'svelte-i18n';
  import type { GatewayStatus } from '$lib/api/gateway';
  import { startGateway, stopGateway } from '$lib/api/gateway';
  
  interface Props {
    status: GatewayStatus;
    onRefresh: () => void;
  }
  
  let { status, onRefresh }: Props = $props();
  let loading = $state(false);
  
  async function handleStart() {
    loading = true;
    try {
      await startGateway();
      onRefresh();
    } finally {
      loading = false;
    }
  }
  
  async function handleStop() {
    loading = true;
    try {
      await stopGateway();
      onRefresh();
    } finally {
      loading = false;
    }
  }
</script>

<div class="glass-card gateway-card">
  <h2>{$_('gateway.status')}</h2>
  <div class="status-content">
    <div class="status-info">
      <span class="status-dot" class:running={status.status === 'running'}></span>
      <span class="status-text">{$_(`gateway.${status.status}`)}</span>
      <span class="version">{status.version}</span>
    </div>
    <div class="actions">
      {#if status.status === 'running'}
        <button class="neon-button stop" onclick={handleStop} disabled={loading}>
          {$_('gateway.stop')}
        </button>
      {:else}
        <button class="neon-button start" onclick={handleStart} disabled={loading}>
          {$_('gateway.start')}
        </button>
      {/if}
      <button class="neon-button" onclick={onRefresh} disabled={loading}>
        🔄
      </button>
    </div>
  </div>
</div>

<style>
  .gateway-card {
    padding: 1.5rem;
  }
  
  .status-content {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 1rem;
  }
  
  .status-info {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  
  .status-dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--neon-pink);
  }
  
  .status-dot.running {
    background: var(--neon-green);
    box-shadow: var(--glow-green);
    animation: pulse 2s infinite;
  }
  
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }
  
  .actions {
    display: flex;
    gap: 0.5rem;
  }
  
  .start {
    border-color: var(--neon-green);
    color: var(--neon-green);
  }
  
  .start:hover {
    box-shadow: var(--glow-green);
  }
  
  .stop {
    border-color: var(--neon-pink);
    color: var(--neon-pink);
  }
  
  .stop:hover {
    box-shadow: 0 0 20px rgba(255, 0, 110, 0.5);
  }
</style>
```

**Step 3: Create StatsCard component**

Create `src/lib/components/Dashboard/StatsCard.svelte`:
```svelte
<script lang="ts">
  interface Props {
    title: string;
    value: string | number;
    trend?: { value: number; positive: boolean };
    icon?: string;
  }
  
  let { title, value, trend, icon }: Props = $props();
</script>

<div class="glass-card stats-card">
  <div class="header">
    {#if icon}
      <span class="icon">{icon}</span>
    {/if}
    <span class="title">{title}</span>
  </div>
  <div class="value">{typeof value === 'number' ? value.toLocaleString() : value}</div>
  {#if trend}
    <div class="trend" class:positive={trend.positive}>
      {trend.positive ? '↑' : '↓'} {Math.abs(trend.value)}%
    </div>
  {/if}
</div>

<style>
  .stats-card {
    padding: 1.25rem;
  }
  
  .header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.75rem;
    color: var(--text-secondary);
    font-size: 0.875rem;
  }
  
  .icon {
    font-size: 1.25rem;
  }
  
  .value {
    font-size: 1.75rem;
    font-weight: 700;
    color: var(--neon-cyan);
  }
  
  .trend {
    font-size: 0.875rem;
    margin-top: 0.5rem;
    color: var(--neon-pink);
  }
  
  .trend.positive {
    color: var(--neon-green);
  }
</style>
```

**Step 4: Create Dashboard page**

Create `src/routes/+page.svelte`:
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { getGatewayStatus, type GatewayStatus } from '$lib/api/gateway';
  import GatewayCard from '$lib/components/Dashboard/GatewayCard.svelte';
  import StatsCard from '$lib/components/Dashboard/StatsCard.svelte';
  
  let gatewayStatus = $state<GatewayStatus>({ status: 'unknown', version: '', pid: null });
  let stats = $state({
    tokens: 12345,
    apiCalls: 234,
    tasks: 15,
  });
  
  async function loadStatus() {
    gatewayStatus = await getGatewayStatus();
  }
  
  onMount(() => {
    loadStatus();
    // Poll every 5 seconds
    const interval = setInterval(loadStatus, 5000);
    return () => clearInterval(interval);
  });
</script>

<div class="dashboard">
  <h1 class="page-title">{$_('app.welcome')}</h1>
  
  <div class="cards-row">
    <div class="card-wide">
      <GatewayCard status={gatewayStatus} onRefresh={loadStatus} />
    </div>
    <div class="cards-grid">
      <StatsCard title="Token" value={stats.tokens} trend={{ value: 15, positive: true }} icon="🪙" />
      <StatsCard title="API Calls" value={stats.apiCalls} trend={{ value: 8, positive: true }} icon="📡" />
    </div>
  </div>
  
  <div class="section">
    <h2>Recent Sessions</h2>
    <div class="sessions-grid">
      <!-- Recent sessions will go here -->
    </div>
  </div>
  
  <div class="section">
    <h2>Quick Actions</h2>
    <div class="quick-actions">
      <button class="neon-button">💬 New Chat</button>
      <button class="neon-button">📋 New Task</button>
      <button class="neon-button">🧩 Browse Skills</button>
      <button class="neon-button">📊 View Monitor</button>
    </div>
  </div>
</div>

<style>
  .dashboard {
    max-width: 1200px;
  }
  
  .page-title {
    font-size: 1.75rem;
    margin-bottom: 1.5rem;
  }
  
  .cards-row {
    display: flex;
    gap: 1.5rem;
    margin-bottom: 1.5rem;
  }
  
  .card-wide {
    flex: 1;
    min-width: 300px;
  }
  
  .cards-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 1rem;
    flex: 1;
  }
  
  .section {
    margin-top: 2rem;
  }
  
  .section h2 {
    font-size: 1.25rem;
    margin-bottom: 1rem;
    color: var(--text-secondary);
  }
  
  .quick-actions {
    display: flex;
    gap: 1rem;
    flex-wrap: wrap;
  }
</style>
```

**Step 5: Verify Dashboard**

Run: `npm run tauri dev`
Expected: Dashboard shows gateway status, stats cards, quick actions

**Commit:** `git add -A && git commit -m "feat: create dashboard page with gateway status and stats"`

---

## Phase 5: Auto-Install Flow

### Task 7: Create Installation Wizard

**Files:**
- Create: `src/lib/stores/install.ts`
- Create: `src/lib/components/Install/InstallWizard.svelte`
- Create: `src/lib/api/install.ts`
- Modify: `src/routes/+layout.svelte`

**Step 1: Create install API**

Create `src/lib/api/install.ts`:
```typescript
import { invoke } from '@tauri-apps/api/core';

export interface SystemCheck {
  nodejs: ComponentStatus;
  openclaw: ComponentStatus;
  platform: string;
  is_china: boolean;
}

export interface ComponentStatus {
  installed: boolean;
  version: string | null;
}

export async function checkSystem(): Promise<SystemCheck> {
  return invoke('check_system');
}

export async function installOpenclaw(useMirror: boolean): Promise<string> {
  return invoke('install_openclaw', { useMirror });
}
```

**Step 2: Create install store**

Create `src/lib/stores/install.ts`:
```svelte
<script lang="ts">
  import type { SystemCheck } from '$lib/api/install';
  
  let systemCheck = $state<SystemCheck | null>(null);
  let isChecking = $state(true);
  let needsInstall = $state(false);
  let installProgress = $state(0);
  let installLog = $state<string[]>([]);
  let installComplete = $state(false);
  
  export function getInstallState() {
    return {
      systemCheck,
      isChecking,
      needsInstall,
      installProgress,
      installLog,
      installComplete,
    };
  }
  
  export function setSystemCheck(check: SystemCheck) {
    systemCheck = check;
    isChecking = false;
    needsInstall = !check.openclaw.installed;
  }
  
  export function addLog(line: string) {
    installLog = [...installLog, line];
  }
  
  export function setProgress(progress: number) {
    installProgress = progress;
  }
  
  export function completeInstall() {
    installComplete = true;
    needsInstall = false;
  }
</script>
```

**Step 3: Create InstallWizard component**

Create `src/lib/components/Install/InstallWizard.svelte`:
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { checkSystem, installOpenclaw, type SystemCheck } from '$lib/api/install';
  
  let systemCheck = $state<SystemCheck | null>(null);
  let isChecking = $state(true);
  let acceptedTerms = $state(false);
  let isInstalling = $state(false);
  let installLog = $state<string[]>([]);
  let installComplete = $state(false);
  let error = $state('');
  
  onMount(async () => {
    systemCheck = await checkSystem();
    isChecking = false;
  });
  
  async function handleInstall() {
    if (!systemCheck) return;
    
    isInstalling = true;
    installLog = [];
    error = '';
    
    try {
      installLog.push('Starting installation...');
      installLog.push(`Using ${systemCheck.is_china ? 'China mirror' : 'default registry'}...`);
      
      const result = await installOpenclaw(systemCheck.is_china);
      installLog.push(result);
      installComplete = true;
    } catch (e) {
      error = String(e);
      installLog.push(`Error: ${e}`);
    } finally {
      isInstalling = false;
    }
  }
</script>

<div class="install-wizard">
  <div class="wizard-content">
    {#if isChecking}
      <div class="checking">
        <div class="spinner"></div>
        <p>Detecting your system environment...</p>
      </div>
    {:else if installComplete}
      <div class="complete">
        <div class="success-icon">✅</div>
        <h2>Installation Complete!</h2>
        <div class="components">
          <div class="component">
            <span class="status installed">✅</span>
            <span>Node.js</span>
            <span class="version">{systemCheck?.nodejs.version}</span>
          </div>
          <div class="component">
            <span class="status installed">✅</span>
            <span>OpenClaw</span>
            <span class="version">Installed</span>
          </div>
        </div>
        <button class="neon-button primary" onclick={() => window.location.reload()}>
          Start Using ClawBox
        </button>
      </div>
    {:else if !systemCheck?.openclaw.installed}
      <div class="need-install">
        <h2>🔧 Environment Setup</h2>
        <p>The following components need to be installed:</p>
        
        <div class="components">
          <div class="component">
            <span class:installed={systemCheck?.nodejs.installed}>
              {systemCheck?.nodejs.installed ? '✅' : '❌'}
            </span>
            <span>Node.js</span>
            <span class="version">{systemCheck?.nodejs.version || 'Not installed'}</span>
          </div>
          <div class="component">
            <span class="status not-installed">❌</span>
            <span>OpenClaw</span>
            <span class="version">Not installed</span>
          </div>
        </div>
        
        <div class="will-install">
          <p>Will install:</p>
          <ul>
            <li>OpenClaw CLI {systemCheck?.is_china ? '(using China mirror)' : ''}</li>
          </ul>
        </div>
        
        <label class="terms">
          <input type="checkbox" bind:checked={acceptedTerms} />
          <span>I have read and agree to the Terms of Service</span>
        </label>
        
        {#if error}
          <div class="error">{error}</div>
        {/if}
        
        <div class="actions">
          <button class="neon-button" onclick={() => window.close()}>Skip</button>
          <button 
            class="neon-button primary" 
            onclick={handleInstall}
            disabled={!acceptedTerms || isInstalling}
          >
            {isInstalling ? 'Installing...' : 'Start Installation'}
          </button>
        </div>
        
        {#if installLog.length > 0}
          <div class="log">
            {#each installLog as line}
              <div class="log-line">{line}</div>
            {/each}
          </div>
        {/if}
      </div>
    {:else}
      <slot />
    {/if}
  </div>
</div>

<style>
  .install-wizard {
    position: fixed;
    inset: 0;
    background: var(--bg-primary);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  
  .wizard-content {
    width: 100%;
    max-width: 500px;
    padding: 2rem;
  }
  
  .checking {
    text-align: center;
  }
  
  .spinner {
    width: 48px;
    height: 48px;
    border: 4px solid var(--bg-tertiary);
    border-top-color: var(--neon-cyan);
    border-radius: 50%;
    animation: spin 1s linear infinite;
    margin: 0 auto 1rem;
  }
  
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  
  .complete {
    text-align: center;
  }
  
  .success-icon {
    font-size: 4rem;
    margin-bottom: 1rem;
  }
  
  .components {
    margin: 1.5rem 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  
  .component {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.75rem 1rem;
    background: var(--bg-tertiary);
    border-radius: 0.5rem;
  }
  
  .version {
    margin-left: auto;
    color: var(--text-muted);
    font-size: 0.875rem;
  }
  
  .terms {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    margin: 1.5rem 0;
    cursor: pointer;
  }
  
  .actions {
    display: flex;
    gap: 1rem;
    justify-content: center;
  }
  
  .primary {
    background: var(--neon-cyan);
    color: var(--bg-primary);
    font-weight: 600;
  }
  
  .primary:hover {
    box-shadow: var(--glow-cyan);
  }
  
  .log {
    margin-top: 1.5rem;
    padding: 1rem;
    background: var(--bg-primary);
    border-radius: 0.5rem;
    font-family: monospace;
    font-size: 0.875rem;
    max-height: 200px;
    overflow-y: auto;
  }
  
  .log-line {
    color: var(--text-secondary);
    margin: 0.25rem 0;
  }
  
  .error {
    color: var(--neon-pink);
    margin: 1rem 0;
    padding: 0.75rem;
    background: rgba(255, 0, 110, 0.1);
    border-radius: 0.5rem;
  }
</style>
```

**Step 4: Integrate into layout**

Modify `src/routes/+layout.svelte` to check installation on mount.

**Step 5: Verify installation wizard**

Run: `npm run tauri dev`
Expected: If OpenClaw not installed, shows installation wizard

**Commit:** `git add -A && git commit -m "feat: add auto-install wizard with system detection"`

---

## Remaining Tasks (Summary)

The following tasks follow the same pattern. Each should be implemented as a separate task with:
1. Create/update files
2. Implement components/logic
3. Test
4. Commit

### Task 8: Chat Module
- Multi-tab chat interface
- Message rendering with Markdown
- Stream output support

### Task 9: Config Management
- Model/Channel/Agent/Skill config tabs
- Form components for each config type

### Task 10: Agent Collaboration
- Flow editor with drag-drop nodes
- Message trace timeline

### Task 11: Monitor Module
- Trace waterfall view
- Resource usage charts

### Task 12: Task Management
- Task list with enable/disable
- Visual flow editor
- User-friendly schedule picker

### Task 13: Logs Module
- File list sidebar
- Content viewer with filtering

### Task 14: Skills Store
- Category sidebar
- Skill cards grid
- Install/uninstall actions

### Task 15: About Module
- Version display
- Update checker

### Task 16: Build & Package
- Configure Tauri for production
- Code signing setup
- Build for macOS/Windows/Linux

---

## Build Commands

```bash
# Development
npm run tauri dev

# Build
npm run tauri build

# Build for specific platform
npm run tauri build -- --target aarch64-apple-darwin
```

## Testing Strategy

- Unit tests for Rust commands
- Component tests for Svelte components
- E2E tests for critical user flows
- Manual testing on each platform

## Deployment

- GitHub Actions for CI/CD
- Auto-update via Tauri's updater
- Release notes generation
