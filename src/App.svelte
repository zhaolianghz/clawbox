<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import Sidebar from './lib/components/Sidebar.svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import LogsPage from './routes/logs/+page.svelte';
  import AboutPage from './routes/about/+page.svelte';
  import AgentsPage from './routes/agents/+page.svelte';
  import MonitorPage from './routes/monitor/+page.svelte';
  import TasksPage from './routes/tasks/+page.svelte';
  import ConfigPage from './routes/config/+page.svelte';
  import CapabilitiesPage from './routes/capabilities/+page.svelte';
  import { openclawGateway } from './lib/api/chat';
  import { get_stats, extractMetrics, type DashboardMetrics } from './lib/api/stats';
  import { list_gateway_statuses } from './lib/api/gateway';
  import { list_backends, type BackendInfo } from './lib/api/backends';

  let currentPage = $state('home');
  let gatewayStatus = $state('stopped');
  let gatewayVersion = $state('v1.0.0');
  let tokenCount = $state(0);
  let metrics = $state<DashboardMetrics | null>(null);
  let statsLoading = $state(true);
  let backends = $state<BackendInfo[]>([]);
  let gatewayByBackend = $state<Record<string, { status: 'running' | 'stopped'; version: string; pid?: number }>>({});
  let messages: {role: string, content: string, id?: string}[] = $state([]);
  let inputValue = $state('');
  let isLoading = $state(false);
  
  let isUpdating = $state(false);
  let updateProgress = $state(0);
  let updateStatus = $state('');

  function navigate(event: CustomEvent<string>) {
    currentPage = event.detail;
  }

  async function loadStats() {
    statsLoading = true;
    try {
      const stats = await get_stats(30);
      metrics = extractMetrics(stats);
      tokenCount = metrics.totalTokens ?? 0;
    } catch (e) {
      console.error('Failed to load stats:', e);
      metrics = null;
    } finally {
      statsLoading = false;
    }
  }

  async function loadBackends() {
    try {
      const [bl, gs] = await Promise.all([list_backends(), list_gateway_statuses()]);
      backends = bl;
      const map: typeof gatewayByBackend = {};
      for (const s of gs.statuses) map[s.backend] = s.status;
      gatewayByBackend = map;
      // Pick first installed backend whose gateway is actually running, else first installed.
      let pick: BackendInfo | undefined;
      for (const b of bl) {
        if (!b.installed) continue;
        const running = map[b.id]?.status === 'running';
        if (running) { pick = b; break; }
      }
      if (!pick) pick = bl.find((b) => b.installed);
      if (pick) {
        const s = map[pick.id];
        gatewayStatus = s?.status ?? 'stopped';
        gatewayVersion = s?.version ?? pick.version ?? 'v1.0.0';
      }
    } catch (e) {
      console.error('Failed to load backends:', e);
    }
  }

  onMount(async () => {
    console.log('App mounted');
    try {
      await loadBackends();
    } catch (e) {
      console.error('Backend error:', e);
      gatewayStatus = 'stopped';
    }

    await loadStats();

    // Connect to gateway for chat
    if (gatewayStatus === 'running') {
      try {
        await openclawGateway.connect(
          (msg) => {
            messages = [...messages, { role: msg.role, content: msg.content, id: msg.id }];
          },
          (_status) => {
            // Status updates from chat connection (not currently surfaced in UI).
          }
        );
        const history = await openclawGateway.getHistory();
        if (history.length > 0) {
          messages = history.map(m => ({ role: m.role, content: m.content, id: m.id }));
        }
      } catch (err) {
        console.error('Failed to connect to gateway for chat:', err);
      }
    }
  });

  async function sendMessage() {
    if (!inputValue.trim() || isLoading) return;
    const userMsg = { role: 'user', content: inputValue, id: Date.now().toString() };
    messages = [...messages, userMsg];
    const currentInput = inputValue;
    inputValue = '';
    isLoading = true;
    
    try {
      const response = await openclawGateway.sendMessage(currentInput);
      messages = [...messages, { role: response.role, content: response.content, id: response.id }];
    } catch (err) {
      messages = [...messages, { role: 'assistant', content: `Error: ${err instanceof Error ? err.message : 'Failed to send message'}`, id: Date.now().toString() }];
    } finally {
      isLoading = false;
    }
  }

  async function updateGateway() {
    if (isUpdating) return;
    isUpdating = true;
    updateProgress = 0;
    updateStatus = 'Checking for updates...';

    try {
      // 1. 检查版本
      const result = await invoke<{
        has_update: boolean;
        current_version: string;
        latest_version: string | null;
        message: string;
      }>('check_openclaw_update');

      // 2. 如果没有更新
      if (!result.has_update) {
        updateProgress = 100;
        updateStatus = result.message;
        setTimeout(() => {
          isUpdating = false;
          updateProgress = 0;
          updateStatus = '';
        }, 3000);
        return;
      }

      // 3. 有更新，开始安装
      updateProgress = 10;
      updateStatus = 'Downloading update...';

      try {
        await invoke('install_openclaw', { useMirror: false });
      } catch (e) {
        // 安装命令可能返回异步结果，继续等待
      }

      // 4. 模拟安装进度
      for (let i = 20; i <= 80; i += 20) {
        await new Promise(resolve => setTimeout(resolve, 500));
        updateProgress = i;
        updateStatus = i === 40 ? 'Installing...' : i === 60 ? 'Verifying...' : updateStatus;
      }

      // 5. 验证安装
      updateProgress = 90;
      updateStatus = 'Verifying installation...';

      // 等待一下让 npm 完成安装
      await new Promise(resolve => setTimeout(resolve, 1500));

      // 6. 再次检查版本确认更新成功
      const afterUpdate = await invoke<{
        has_update: boolean;
        current_version: string;
        latest_version: string | null;
        message: string;
      }>('check_openclaw_update');

      updateProgress = 100;

      if (afterUpdate.has_update) {
        updateStatus = 'Update failed. Please try again.';
      } else {
        updateStatus = `Updated to ${afterUpdate.current_version}!`;
      }

      setTimeout(() => {
        isUpdating = false;
        updateProgress = 0;
        updateStatus = '';
      }, 3000);

    } catch (e) {
      updateStatus = 'Failed to check for updates';
      setTimeout(() => {
        isUpdating = false;
        updateProgress = 0;
        updateStatus = '';
      }, 3000);
    }
  }
</script>

<div class="app">
  <Sidebar activeItem={currentPage} onnavigate={navigate} />
  
  <main class="main-area">
    <TopBar />
    <div class="content">
      {#if currentPage === 'home'}
        <div class="dashboard">
          <h1>Dashboard</h1>
          <div class="gateway-status glass-card">
            <div class="gateway-header">
              <div>
                <h2>Gateway Status</h2>
                <div class="gateway-row">
                  {#each backends.filter((b) => b.installed) as b (b.id)}
                    <div class="gateway-card">
                      <span class="backend-chip" data-backend={b.id}>{b.displayName}</span>
                      <span class="version">{b.version}</span>
                      <span class="status" class:running={gatewayByBackend[b.id]?.status === 'running'}>
                        {gatewayByBackend[b.id]?.status ?? 'unknown'}
                      </span>
                    </div>
                  {/each}
                </div>
              </div>
              <button class="neon-button update-btn" onclick={updateGateway} disabled={isUpdating}>
                {#if isUpdating}
                  <span class="update-spinner"></span>
                  Updating...
                {:else}
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M21 12a9 9 0 11-9-9"/>
                    <polyline points="21 3 21 9 15 9"/>
                  </svg>
                  Check Update
                {/if}
              </button>
            </div>
            
            {#if isUpdating}
              <div class="update-progress">
                <div class="progress-header">
                  <span>{updateStatus}</span>
                  <span>{updateProgress}%</span>
                </div>
                <div class="progress-bar">
                  <div class="progress-fill" style="width: {updateProgress}%"></div>
                </div>
              </div>
            {/if}
          </div>
          {#if statsLoading}
            <div class="stats-row">
              <div class="stat-card glass-card"><span class="stat-value muted">…</span><span class="stat-label">Tokens (30d)</span></div>
              <div class="stat-card glass-card"><span class="stat-value muted">…</span><span class="stat-label">API Calls</span></div>
              <div class="stat-card glass-card"><span class="stat-value muted">…</span><span class="stat-label">Cost (USD)</span></div>
            </div>
          {:else if metrics && metrics.gatewayRunning}
            <div class="stats-row">
              <div class="stat-card glass-card">
                <span class="stat-value">{metrics.totalTokens?.toLocaleString() ?? '—'}</span>
                <span class="stat-label">Tokens (30d)</span>
              </div>
              <div class="stat-card glass-card">
                <span class="stat-value">{metrics.apiCalls?.toLocaleString() ?? '—'}</span>
                <span class="stat-label">API Calls</span>
              </div>
              <div class="stat-card glass-card">
                <span class="stat-value">{metrics.totalCost != null ? '$' + metrics.totalCost.toFixed(2) : '—'}</span>
                <span class="stat-label">Cost (USD)</span>
              </div>
            </div>
          {:else}
            <div class="stats-empty glass-card">
              <span class="empty-icon">📊</span>
              <p>Start the gateway to see usage statistics.</p>
            </div>
          {/if}
        </div>
      {:else if currentPage === 'chat'}
        <div class="chat-container">
          <div class="chat-messages">
            {#each messages as msg}
              <div class="message {msg.role}">
                <p>{msg.content}</p>
              </div>
            {/each}
          </div>
          <div class="chat-input-area">
            <input type="text" bind:value={inputValue} placeholder="Type a message..." onkeydown={(e) => e.key === 'Enter' && sendMessage()} disabled={isLoading} />
            <button class="send-btn" onclick={sendMessage} disabled={isLoading || !inputValue.trim()}>
              {#if isLoading}
                <span class="chat-spinner"></span>
              {:else}
                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="22" y1="2" x2="11" y2="13"/>
                  <polygon points="22 2 15 22 11 13 2 9 22 2"/>
                </svg>
              {/if}
            </button>
          </div>
        </div>
      {:else if currentPage === 'config'}
        <ConfigPage />
      {:else if currentPage === 'agents'}
        <AgentsPage />
      {:else if currentPage === 'monitor'}
        <MonitorPage />
      {:else if currentPage === 'tasks'}
        <TasksPage />
      {:else if currentPage === 'logs'}
        <LogsPage />
      {:else if currentPage === 'capabilities'}
        <CapabilitiesPage />
      {:else if currentPage === 'about'}
        <AboutPage />
      {:else}
        <div class="dashboard">
          <h1>Welcome to ClawBox</h1>
          <p>Select a section from the sidebar to get started.</p>
        </div>
      {/if}
    </div>
    <StatusBar {gatewayStatus} {gatewayVersion} {tokenCount} />
  </main>
</div>

<style>
  :global(html, body) {
    margin: 0;
    padding: 0;
    width: 100%;
    height: 100%;
    background-color: #0a0a0f !important;
  }
  
  .app {
    display: flex;
    height: 100vh;
    background-color: #0a0a0f;
    overflow: hidden;
  }
  
  .main-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  
  .content {
    flex: 1;
    overflow: auto;
    padding: 1.5rem;
  }
  
  .dashboard h1 {
    color: var(--neon-cyan);
    text-shadow: var(--glow-cyan);
    margin: 0 0 1.5rem;
  }
  
  .gateway-status {
    padding: 1.5rem;
    margin-bottom: 1.5rem;
  }
  
  .gateway-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
  }
  
  .gateway-status h2 {
    margin: 0 0 0.5rem;
    color: var(--text-primary);
  }
  
  .gateway-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }
  .gateway-card {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    background: var(--bg-primary);
    border-radius: 0.5rem;
    font-size: 0.75rem;
  }
  .backend-chip {
    padding: 0.125rem 0.5rem;
    border-radius: 999px;
    font-weight: 600;
    font-size: 0.7rem;
  }
  .backend-chip[data-backend="openclaw"] { background: rgba(0,245,255,0.15); color: var(--neon-cyan); }
  .backend-chip[data-backend="hermes"]   { background: rgba(255,0,200,0.15); color: #ff6ad5; }
  .gateway-card .version { color: var(--text-muted); }
  .gateway-card .status { font-weight: 600; }
  .gateway-card .status.running { color: var(--neon-green); }

  .gateway-info {
    margin: 0.25rem 0;
    color: var(--text-secondary);
    font-size: 0.9rem;
  }
  
  .update-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  
  .update-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(0, 245, 255, 0.3);
    border-top-color: var(--neon-cyan);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  
  .update-progress {
    margin-top: 1.5rem;
    padding-top: 1rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .progress-header {
    display: flex;
    justify-content: space-between;
    margin-bottom: 0.5rem;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }
  
  .progress-bar {
    height: 8px;
    background: var(--bg-tertiary);
    border-radius: 4px;
    overflow: hidden;
  }
  
  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--neon-cyan), var(--neon-purple));
    border-radius: 4px;
    transition: width 0.3s ease;
  }
  
  .running { color: var(--neon-green); }
  .stopped { color: var(--neon-pink); }
  
  .stats-row {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1rem;
  }
  
  .stat-card {
    padding: 1.5rem;
    text-align: center;
  }
  
  .stat-value {
    display: block;
    font-size: 2rem;
    font-weight: 700;
    color: var(--neon-cyan);
  }
  
  .stat-label {
    display: block;
    margin-top: 0.5rem;
    color: var(--text-muted);
    font-size: 0.875rem;
  }

  .stat-value.muted {
    color: var(--text-muted);
  }

  .stats-empty {
    padding: 2rem;
    text-align: center;
    color: var(--text-muted);
  }

  .stats-empty .empty-icon {
    font-size: 2rem;
    display: block;
    margin-bottom: 0.5rem;
    opacity: 0.6;
  }

  .stats-empty p {
    margin: 0;
  }
  
  .chat-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    gap: 1rem;
  }
  
  .chat-messages {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  
  .message {
    padding: 0.75rem 1rem;
    border-radius: 0.75rem;
    max-width: 80%;
  }
  
  .message.user {
    align-self: flex-end;
    background: rgba(0, 245, 255, 0.15);
    border: 1px solid var(--neon-cyan);
  }
  
  .message.assistant {
    align-self: flex-start;
    background: var(--bg-secondary);
    border: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .message p { margin: 0; }
  
  .chat-input-area {
    display: flex;
    gap: 0.75rem;
  }
  
  .chat-input-area input {
    flex: 1;
    padding: 0.75rem 1rem;
    background: var(--bg-tertiary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 0.5rem;
    color: var(--text-primary);
    font-size: 0.9rem;
  }
  
  .chat-input-area input:focus {
    outline: none;
    border-color: var(--neon-cyan);
  }
  
  .chat-input-area .send-btn {
    width: 44px;
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, var(--neon-cyan) 0%, var(--neon-purple) 100%);
    border: none;
    border-radius: 50%;
    color: white;
    cursor: pointer;
    transition: all 0.2s ease;
    flex-shrink: 0;
  }
  
  .chat-input-area .send-btn:hover:not(:disabled) {
    box-shadow: var(--glow-cyan);
    transform: scale(1.05);
  }

  .chat-spinner {
    width: 16px;
    height: 16px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
