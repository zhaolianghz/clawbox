<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import Sidebar from './lib/components/Sidebar.svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import LogsPage from './routes/logs/+page.svelte';
  import SkillsPage from './routes/skills/+page.svelte';
  import AboutPage from './routes/about/+page.svelte';
  import AgentsPage from './routes/agents/+page.svelte';
  import MonitorPage from './routes/monitor/+page.svelte';
  import TasksPage from './routes/tasks/+page.svelte';
  import ConfigPage from './routes/config/+page.svelte';

  let currentPage = $state('home');
  let gatewayStatus = $state('stopped');
  let gatewayVersion = $state('v1.0.0');
  let tokenCount = $state(12543);
  let messages: {role: string, content: string}[] = $state([]);
  let inputValue = $state('');

  function navigate(event: CustomEvent<string>) {
    currentPage = event.detail;
  }

  onMount(async () => {
    console.log('App mounted');
    try {
      const status = await invoke<{ running: boolean; version: string }>('get_gateway_status');
      gatewayStatus = status.running ? 'running' : 'stopped';
      gatewayVersion = status.version || 'v1.0.0';
    } catch (e) {
      console.error('Gateway error:', e);
      gatewayStatus = 'stopped';
    }
  });

  function sendMessage() {
    if (!inputValue.trim()) return;
    messages = [...messages, { role: 'user', content: inputValue }];
    inputValue = '';
    setTimeout(() => {
      messages = [...messages, { role: 'assistant', content: 'This is a mock response.' }];
    }, 1000);
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
            <h2>Gateway Status</h2>
            <p>Status: <span class:running={gatewayStatus === 'running'} class:stopped={gatewayStatus === 'stopped'}>{gatewayStatus}</span></p>
            <p>Version: {gatewayVersion}</p>
          </div>
          <div class="stats-row">
            <div class="stat-card glass-card">
              <span class="stat-value">{tokenCount.toLocaleString()}</span>
              <span class="stat-label">Tokens Today</span>
            </div>
            <div class="stat-card glass-card">
              <span class="stat-value">847</span>
              <span class="stat-label">API Calls</span>
            </div>
            <div class="stat-card glass-card">
              <span class="stat-value">23</span>
              <span class="stat-label">Tasks</span>
            </div>
          </div>
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
            <input type="text" bind:value={inputValue} placeholder="Type a message..." onkeydown={(e) => e.key === 'Enter' && sendMessage()} />
            <button onclick={sendMessage}>Send</button>
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
      {:else if currentPage === 'skills'}
        <SkillsPage />
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
  
  .gateway-status h2 {
    margin: 0 0 1rem;
    color: var(--text-primary);
  }
  
  .gateway-status p {
    margin: 0.5rem 0;
    color: var(--text-secondary);
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
  
  .chat-input-area button {
    padding: 0.75rem 1.5rem;
    background: var(--neon-cyan);
    border: none;
    border-radius: 0.5rem;
    color: var(--bg-primary);
    font-weight: 600;
    cursor: pointer;
  }
  
  .chat-input-area button:hover {
    box-shadow: var(--glow-cyan);
  }
</style>
