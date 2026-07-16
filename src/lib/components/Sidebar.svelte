<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  
  interface Props {
    activeItem?: string;
    onnavigate?: (event: CustomEvent<string>) => void;
  }
  
  let { activeItem = 'home', onnavigate }: Props = $props();
  
  let isReady = $state(false);
  
  const navItems = [
    { id: 'home', label: 'Home' },
    { id: 'chat', label: 'Chat' },
    { id: 'config', label: 'Config' },
    { id: 'agents', label: 'Agents' },
    { id: 'monitor', label: 'Monitor' },
    { id: 'tasks', label: 'Tasks' },
    { id: 'capabilities', label: 'Capabilities' },
  ];
  
  onMount(() => {
    isReady = true;
  });
  
  function handleClick(id: string) {
    activeItem = id;
    onnavigate?.(new CustomEvent('navigate', { detail: id }));
  }
  
  function t(key: string): string {
    if (!isReady) {
      const labels: Record<string, string> = {
        home: 'Home', chat: 'Chat', config: 'Config', agents: 'Agents',
        monitor: 'Monitor', tasks: 'Tasks', capabilities: 'Capabilities', about: 'About'
      };
      return labels[key] || key;
    }
    return $_(`nav.${key}`);
  }
</script>

<aside class="sidebar">
  <nav class="nav-main">
    {#each navItems as item}
      <button
        class="nav-item"
        class:active={activeItem === item.id}
        onclick={() => handleClick(item.id)}
      >
        {#if item.id === 'home'}
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>
            <polyline points="9 22 9 12 15 12 15 22"/>
          </svg>
        {:else if item.id === 'chat'}
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
          </svg>
        {:else if item.id === 'config'}
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="3"/>
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
          </svg>
        {:else if item.id === 'agents'}
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
            <circle cx="8.5" cy="8.5" r="1.5"/>
            <polyline points="21 15 16 10 5 21"/>
          </svg>
        {:else if item.id === 'monitor'}
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="20" x2="18" y2="10"/>
            <line x1="12" y1="20" x2="12" y2="4"/>
            <line x1="6" y1="20" x2="6" y2="14"/>
          </svg>
        {:else if item.id === 'tasks'}
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M9 11l3 3L22 4"/>
            <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/>
          </svg>
        {:else if item.id === 'capabilities'}
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12 2L2 7l10 5 10-5-10-5z"/>
            <path d="M2 17l10 5 10-5"/>
            <path d="M2 12l10 5 10-5"/>
          </svg>
        {/if}
        <span class="label">{t(item.id)}</span>
      </button>
    {/each}
  </nav>
  
  <nav class="nav-bottom">
    <button
      class="nav-item"
      class:active={activeItem === 'about'}
      onclick={() => handleClick('about')}
    >
      <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10"/>
        <line x1="12" y1="16" x2="12" y2="12"/>
        <line x1="12" y1="8" x2="12.01" y2="8"/>
      </svg>
      <span class="label">{t('about')}</span>
    </button>
  </nav>
</aside>

<style>
  .sidebar {
    width: 200px;
    height: 100%;
    background: var(--bg-secondary);
    border-right: 1px solid rgba(255, 255, 255, 0.1);
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 1rem 0;
  }
  
  .nav-main, .nav-bottom {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0 0.5rem;
  }
  
  .nav-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    background: transparent;
    border: none;
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    font-size: 14px;
    cursor: pointer;
    transition: all 0.2s ease;
    width: 100%;
  }
  
  .nav-item :global(svg) {
    opacity: 0.6;
    transition: opacity 0.2s ease;
  }
  
  .nav-item:hover :global(svg) {
    opacity: 0.8;
  }
  
  .nav-item.active :global(svg) {
    opacity: 1;
    color: var(--neon-cyan);
  }
  
  .nav-item:hover {
    background: var(--bg-tertiary);
    color: var(--text-primary);
  }
  
  .nav-item.active {
    background: rgba(0, 245, 255, 0.1);
    color: var(--neon-cyan);
    box-shadow: var(--glow-cyan);
  }
  
  .nav-icon {
    width: 20px;
    height: 20px;
    flex-shrink: 0;
  }
  
  .nav-item .label {
    flex: 1;
  }
</style>
