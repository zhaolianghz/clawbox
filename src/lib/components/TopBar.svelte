<script lang="ts">
  import { _ } from 'svelte-i18n';
</script>

<header class="topbar">
  <div class="logo">
    <span class="logo-icon">🦀</span>
    <span class="logo-text">ClawBox</span>
  </div>
  
  <div class="actions">
    <button class="action-btn" title={$_('actions.notifications')}>
      🔔
    </button>
    <button class="action-btn" title={$_('actions.settings')}>
      ⚙️
    </button>
    <button class="action-btn avatar" title={$_('actions.user')}>
      👤
    </button>
  </div>
</header>

<style>
  .topbar {
    height: 60px;
    background: var(--bg-secondary);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 1.5rem;
  }
  
  .logo {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  
  .logo-icon {
    font-size: 1.5rem;
  }
  
  .logo-text {
    font-size: 1.25rem;
    font-weight: 600;
    color: var(--neon-cyan);
    text-shadow: var(--glow-cyan);
  }
  
  .actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  
  .action-btn {
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 0.5rem;
    cursor: pointer;
    transition: all 0.2s ease;
    font-size: 1rem;
  }
  
  .action-btn:hover {
    background: var(--bg-tertiary);
    border-color: rgba(255, 255, 255, 0.1);
  }
  
  .action-btn.avatar {
    border-color: var(--neon-cyan);
  }
</style>
