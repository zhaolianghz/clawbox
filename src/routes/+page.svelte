<script lang="ts">
  import { _ } from 'svelte-i18n';
  import GatewayCard from '$lib/components/Dashboard/GatewayCard.svelte';
  import StatsCard from '$lib/components/Dashboard/StatsCard.svelte';
  
  const mockStats = {
    tokens: '12,450',
    apiCalls: 847,
    tasks: 23
  };
  
  const mockSessions = [
    { id: 1, agent: 'Code Assistant', time: '2 min ago', messages: 12 },
    { id: 2, agent: 'Data Analyzer', time: '15 min ago', messages: 8 },
    { id: 3, agent: 'Research Bot', time: '1 hour ago', messages: 24 },
    { id: 4, agent: 'Code Assistant', time: '3 hours ago', messages: 45 },
  ];
  
  const quickActions = [
    { id: 'new-chat', icon: '💬', labelKey: 'dashboard.newChat' },
    { id: 'new-task', icon: '📋', labelKey: 'dashboard.newTask' },
    { id: 'import-config', icon: '📥', labelKey: 'dashboard.importConfig' },
    { id: 'export-logs', icon: '📤', labelKey: 'dashboard.exportLogs' },
  ];
</script>

<div class="dashboard">
  <header class="page-header">
    <h1>{$_('dashboard.title')}</h1>
    <p class="subtitle">{$_('dashboard.subtitle')}</p>
  </header>
  
  <section class="top-row">
    <GatewayCard />
    
    <div class="stats-row">
      <StatsCard icon="🪙" labelKey="dashboard.tokensToday" value={mockStats.tokens} trend="+12%" />
      <StatsCard icon="📡" labelKey="dashboard.apiCalls" value={mockStats.apiCalls} />
      <StatsCard icon="✅" labelKey="dashboard.tasksCompleted" value={mockStats.tasks} />
    </div>
  </section>
  
  <section class="content-row">
    <div class="sessions-panel glass-card">
      <h2>{$_('dashboard.recentSessions')}</h2>
      <ul class="sessions-list">
        {#each mockSessions as session}
          <li class="session-item">
            <span class="agent">{session.agent}</span>
            <span class="meta">
              <span class="time">{session.time}</span>
              <span class="messages">{session.messages} {$_('dashboard.messages')}</span>
            </span>
          </li>
        {/each}
      </ul>
    </div>
    
    <div class="actions-panel glass-card">
      <h2>{$_('dashboard.quickActions')}</h2>
      <div class="actions-grid">
        {#each quickActions as action}
          <button class="action-btn">
            <span class="icon">{action.icon}</span>
            <span class="label">{$_(action.labelKey)}</span>
          </button>
        {/each}
      </div>
    </div>
  </section>
</div>

<style>
  .dashboard {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }
  
  .page-header h1 {
    margin: 0;
    font-size: 1.5rem;
    color: var(--neon-cyan);
    text-shadow: var(--glow-cyan);
  }
  
  .page-header .subtitle {
    margin: 0.25rem 0 0;
    color: var(--text-secondary);
    font-size: 0.9rem;
  }
  
  .top-row {
    display: flex;
    gap: 1.5rem;
    flex-wrap: wrap;
  }
  
  .stats-row {
    display: flex;
    gap: 1rem;
    flex-wrap: wrap;
    flex: 1;
  }
  
  .content-row {
    display: flex;
    gap: 1.5rem;
    flex-wrap: wrap;
  }
  
  .sessions-panel, .actions-panel {
    flex: 1;
    min-width: 280px;
    padding: 1.25rem;
  }
  
  .sessions-panel h2, .actions-panel h2 {
    margin: 0 0 1rem;
    font-size: 1rem;
    color: var(--text-primary);
  }
  
  .sessions-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  
  .session-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.75rem;
    background: var(--bg-tertiary);
    border-radius: 0.5rem;
    cursor: pointer;
    transition: background 0.2s ease;
  }
  
  .session-item:hover {
    background: rgba(0, 245, 255, 0.1);
  }
  
  .session-item .agent {
    color: var(--text-primary);
    font-size: 0.9rem;
  }
  
  .session-item .meta {
    display: flex;
    gap: 0.75rem;
    font-size: 0.8rem;
    color: var(--text-secondary);
  }
  
  .actions-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 0.75rem;
  }
  
  .action-btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 1rem;
    background: var(--bg-tertiary);
    border: 1px solid transparent;
    border-radius: 0.75rem;
    cursor: pointer;
    transition: all 0.2s ease;
  }
  
  .action-btn:hover {
    background: rgba(0, 245, 255, 0.1);
    border-color: var(--neon-cyan);
  }
  
  .action-btn .icon {
    font-size: 1.5rem;
  }
  
  .action-btn .label {
    font-size: 0.8rem;
    color: var(--text-secondary);
  }
</style>
