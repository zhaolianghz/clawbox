<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  
  let appVersion = $state('0.1.0');
  let gatewayVersion = $state('unknown');
  let checking = $state(false);
  let updateAvailable = $state(false);
  
  async function checkForUpdates() {
    checking = true;
    await new Promise(resolve => setTimeout(resolve, 1500));
    checking = false;
  }
  
  onMount(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const status = await invoke('get_gateway_status') as { version: string };
      gatewayVersion = status.version || 'unknown';
    } catch {
      gatewayVersion = 'not installed';
    }
  });
</script>

<svelte:head>
  <title>{$_('about.title')} - ClawBox</title>
</svelte:head>

<div class="about-page">
  <div class="about-header">
    <div class="logo-large">🎮</div>
    <h1>ClawBox</h1>
    <p class="tagline">{$_('about.tagline')}</p>
  </div>
  
  <div class="about-content">
    <div class="info-section glass-card">
      <h2>{$_('about.versions')}</h2>
      <div class="version-list">
        <div class="version-item">
          <span class="label">ClawBox</span>
          <span class="value">v{appVersion}</span>
        </div>
        <div class="version-item">
          <span class="label">OpenClaw Gateway</span>
          <span class="value">{gatewayVersion}</span>
        </div>
        <div class="version-item">
          <span class="label">Tauri</span>
          <span class="value">v2.0</span>
        </div>
        <div class="version-item">
          <span class="label">Svelte</span>
          <span class="value">v5.0</span>
        </div>
      </div>
      
      <div class="update-section">
        {#if updateAvailable}
          <div class="update-available">
            <span>🎉 {$_('about.updateAvailable')}</span>
            <button class="neon-button">{$_('about.updateNow')}</button>
          </div>
        {:else}
          <button class="neon-button" onclick={checkForUpdates} disabled={checking}>
            {#if checking}
              <span class="spinner-small"></span>
              {$_('about.checking')}
            {:else}
              🔄 {$_('about.checkUpdates')}
            {/if}
          </button>
        {/if}
      </div>
    </div>
    
    <div class="info-section glass-card">
      <h2>{$_('about.links')}</h2>
      <div class="link-list">
        <a href="https://github.com/openclaw/clawbox" class="link-item" target="_blank" rel="noopener">
          <span class="link-icon">📦</span>
          <span class="link-text">GitHub Repository</span>
          <span class="link-arrow">→</span>
        </a>
        <a href="https://docs.openclaw.ai" class="link-item" target="_blank" rel="noopener">
          <span class="link-icon">📚</span>
          <span class="link-text">{$_('about.documentation')}</span>
          <span class="link-arrow">→</span>
        </a>
        <a href="https://github.com/openclaw/clawbox/issues" class="link-item" target="_blank" rel="noopener">
          <span class="link-icon">🐛</span>
          <span class="link-text">{$_('about.reportIssue')}</span>
          <span class="link-arrow">→</span>
        </a>
        <a href="https://discord.gg/openclaw" class="link-item" target="_blank" rel="noopener">
          <span class="link-icon">💬</span>
          <span class="link-text">{$_('about.discord')}</span>
          <span class="link-arrow">→</span>
        </a>
      </div>
    </div>
    
    <div class="info-section glass-card">
      <h2>{$_('about.credits')}</h2>
      <p class="credits-text">
        {$_('about.creditsText')}
      </p>
      <div class="tech-badges">
        <span class="badge">Tauri</span>
        <span class="badge">Svelte</span>
        <span class="badge">Rust</span>
        <span class="badge">TypeScript</span>
        <span class="badge">TailwindCSS</span>
      </div>
    </div>
  </div>
  
  <footer class="about-footer">
    <p>© 2024 OpenClaw Team. {$_('about.allRightsReserved')}</p>
  </footer>
</div>

<style>
  .about-page {
    max-width: 800px;
    margin: 0 auto;
    padding: 2rem;
  }
  
  .about-header {
    text-align: center;
    margin-bottom: 2rem;
  }
  
  .logo-large {
    font-size: 4rem;
    margin-bottom: 1rem;
  }
  
  .about-header h1 {
    font-size: 2rem;
    margin: 0 0 0.5rem;
    color: var(--neon-cyan);
    text-shadow: var(--glow-cyan);
  }
  
  .tagline {
    color: var(--text-secondary);
    margin: 0;
  }
  
  .about-content {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }
  
  .info-section {
    padding: 1.5rem;
  }
  
  .info-section h2 {
    font-size: 1rem;
    font-weight: 600;
    margin: 0 0 1rem;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  
  .version-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  
  .version-item {
    display: flex;
    justify-content: space-between;
    padding: 0.5rem 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }
  
  .version-item:last-child {
    border-bottom: none;
  }
  
  .version-item .label {
    color: var(--text-secondary);
  }
  
  .version-item .value {
    color: var(--neon-cyan);
    font-family: monospace;
  }
  
  .update-section {
    margin-top: 1.5rem;
    padding-top: 1rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .update-available {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem;
    background: rgba(0, 255, 136, 0.1);
    border-radius: 0.5rem;
    color: var(--neon-green);
  }
  
  .link-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  
  .link-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    background: var(--bg-tertiary);
    border-radius: 0.5rem;
    color: var(--text-primary);
    text-decoration: none;
    transition: all 0.2s ease;
  }
  
  .link-item:hover {
    background: rgba(0, 245, 255, 0.1);
    color: var(--neon-cyan);
  }
  
  .link-icon {
    font-size: 1.25rem;
  }
  
  .link-text {
    flex: 1;
  }
  
  .link-arrow {
    opacity: 0.5;
  }
  
  .credits-text {
    color: var(--text-secondary);
    line-height: 1.6;
    margin: 0 0 1rem;
  }
  
  .tech-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  
  .badge {
    background: var(--bg-tertiary);
    padding: 0.25rem 0.75rem;
    border-radius: 1rem;
    font-size: 0.75rem;
    color: var(--text-secondary);
  }
  
  .about-footer {
    text-align: center;
    margin-top: 2rem;
    padding-top: 1rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
    color: var(--text-muted);
    font-size: 0.875rem;
  }
  
  .spinner-small {
    width: 14px;
    height: 14px;
    border: 2px solid var(--bg-tertiary);
    border-top-color: currentColor;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    display: inline-block;
    margin-right: 0.5rem;
  }
  
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
