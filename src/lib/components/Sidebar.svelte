<script lang="ts">
  import { _ } from 'svelte-i18n';
  
  let activeItem = $state('home');
  
  const navItems = [
    { id: 'home', icon: '🏠' },
    { id: 'chat', icon: '💬' },
    { id: 'config', icon: '⚙️' },
    { id: 'agents', icon: '🤖' },
    { id: 'monitor', icon: '📊' },
    { id: 'tasks', icon: '📋' },
    { id: 'logs', icon: '📝' },
    { id: 'skills', icon: '✨' },
  ];
  
  function setActive(id: string) {
    activeItem = id;
  }
</script>

<aside class="sidebar">
  <nav class="nav-main">
    {#each navItems as item}
      <button
        class="nav-item"
        class:active={activeItem === item.id}
        onclick={() => setActive(item.id)}
      >
        <span class="icon">{item.icon}</span>
        <span class="label">{$_(`nav.${item.id}`)}</span>
      </button>
    {/each}
  </nav>
  
  <nav class="nav-bottom">
    <button
      class="nav-item"
      class:active={activeItem === 'about'}
      onclick={() => setActive('about')}
    >
      <span class="icon">ℹ️</span>
      <span class="label">{$_('nav.about')}</span>
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
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    background: transparent;
    border: none;
    border-radius: 0.5rem;
    color: var(--text-secondary);
    cursor: pointer;
    transition: all 0.2s ease;
    text-align: left;
    font-size: 0.9rem;
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
  
  .nav-item .icon {
    font-size: 1.1rem;
    width: 1.5rem;
    text-align: center;
  }
  
  .nav-item .label {
    flex: 1;
  }
</style>
