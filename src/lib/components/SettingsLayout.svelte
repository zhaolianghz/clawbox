<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import ProvidersPage from '../../routes/providers/+page.svelte';
  import ConfigPage from '../../routes/config/+page.svelte';
  import AgentHubPage from '../../routes/agents/+page.svelte';
  import CapabilitiesPage from '../../routes/capabilities/+page.svelte';
  import AboutPage from '../../routes/about/+page.svelte';

  type SectionId = 'providers' | 'config' | 'agents' | 'capabilities' | 'about';

  interface Props {
    /** standalone:作为应用本体渲染(无返回按钮,标题为 ClawBox,追加「关于」节) */
    standalone?: boolean;
    onexit?: () => void;
  }
  let { standalone = false, onexit }: Props = $props();

  const sections: { id: SectionId; labelKey: string }[] = [
    { id: 'providers', labelKey: 'nav.providers' },
    { id: 'config', labelKey: 'nav.config' },
    { id: 'agents', labelKey: 'nav.agents' },
    { id: 'capabilities', labelKey: 'nav.capabilities' },
    ...(standalone ? [{ id: 'about' as SectionId, labelKey: 'nav.about' }] : []),
  ];

  let activeSection = $state<SectionId>('providers');
  // keep-alive:子页首次访问后常驻,切回秒开,避免每次重跑 CLI 探测
  let mounted = $state<Record<SectionId, boolean>>({
    providers: true,
    config: false,
    agents: false,
    capabilities: false,
    about: false,
  });

  function select(id: SectionId) {
    activeSection = id;
    mounted[id] = true;
  }

  onMount(() => {
    // 预热:首屏就绪后静默挂载其余子页,数据提前加载
    setTimeout(() => {
      mounted.config = true;
      mounted.agents = true;
      mounted.capabilities = true;
      if (standalone) mounted.about = true;
    }, 600);
  });
</script>

<div class="settings-layout">
  <aside class="settings-menu">
    {#if !standalone}
      <button class="back-btn" onclick={() => onexit?.()}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="19" y1="12" x2="5" y2="12"/>
          <polyline points="12 19 5 12 12 5"/>
        </svg>
        <span>{$_('nav.back')}</span>
      </button>
    {/if}
    <div class="menu-title">{standalone ? $_('app.name') : $_('nav.settings')}</div>
    <nav class="menu-nav">
      {#each sections as s (s.id)}
        <button
          class="menu-item"
          class:active={activeSection === s.id}
          onclick={() => select(s.id)}
        >
          {#if s.id === 'providers'}
            <svg class="menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <rect x="2" y="3" width="20" height="7" rx="2"/>
              <rect x="2" y="14" width="20" height="7" rx="2"/>
              <line x1="6" y1="6.5" x2="6.01" y2="6.5"/>
              <line x1="6" y1="17.5" x2="6.01" y2="17.5"/>
            </svg>
          {:else if s.id === 'config'}
            <svg class="menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
            </svg>
          {:else if s.id === 'agents'}
            <svg class="menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
              <circle cx="8.5" cy="8.5" r="1.5"/>
              <polyline points="21 15 16 10 5 21"/>
            </svg>
          {:else if s.id === 'capabilities'}
            <svg class="menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 2L2 7l10 5 10-5-10-5z"/>
              <path d="M2 17l10 5 10-5"/>
              <path d="M2 12l10 5 10-5"/>
            </svg>
          {:else if s.id === 'about'}
            <svg class="menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="10"/>
              <line x1="12" y1="16" x2="12" y2="12"/>
              <line x1="12" y1="8" x2="12.01" y2="8"/>
            </svg>
          {/if}
          <span class="menu-label">{$_(s.labelKey)}</span>
        </button>
      {/each}
    </nav>
  </aside>

  <div class="settings-content">
    {#if mounted.providers}
      <div class="pane" hidden={activeSection !== 'providers'}><ProvidersPage /></div>
    {/if}
    {#if mounted.config}
      <div class="pane" hidden={activeSection !== 'config'}><ConfigPage /></div>
    {/if}
    {#if mounted.agents}
      <div class="pane" hidden={activeSection !== 'agents'}><AgentHubPage /></div>
    {/if}
    {#if mounted.capabilities}
      <div class="pane" hidden={activeSection !== 'capabilities'}><CapabilitiesPage /></div>
    {/if}
    {#if mounted.about}
      <div class="pane" hidden={activeSection !== 'about'}><AboutPage /></div>
    {/if}
  </div>
</div>

<style>
  /* 设置页现为全屏覆盖层(填满整个窗口),不再嵌在带 padding 的 .content 里 */
  .settings-layout {
    height: 100%;
    display: flex;
    background: var(--bg-primary);
    overflow: hidden;
  }

  .back-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0.5rem 0.75rem;
    padding: 0.5rem 0.75rem;
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    font-size: 0.8rem;
    cursor: pointer;
    transition: all 0.2s ease;
  }
  .back-btn svg {
    width: 16px;
    height: 16px;
  }
  .back-btn:hover {
    color: var(--neon-cyan);
    border-color: var(--neon-cyan);
    background: rgba(0, 245, 255, 0.08);
  }

  .settings-menu {
    width: 200px;
    flex-shrink: 0;
    background: var(--bg-secondary);
    border-right: 1px solid rgba(255, 255, 255, 0.1);
    padding: 1.25rem 0 0.5rem;
    display: flex;
    flex-direction: column;
  }

  .menu-title {
    padding: 0 1rem 0.75rem;
    margin: 0 0.5rem 0.5rem;
    color: var(--neon-cyan);
    text-shadow: var(--glow-cyan);
    font-size: 1rem;
    font-weight: 600;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .menu-nav {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0.75rem 0.5rem 0;
  }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0.6rem 0.85rem;
    background: transparent;
    border: none;
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    font-size: 0.875rem;
    cursor: pointer;
    transition: all 0.2s ease;
    width: 100%;
    text-align: left;
  }

  .menu-icon {
    width: 18px;
    height: 18px;
    opacity: 0.6;
    flex-shrink: 0;
    transition: opacity 0.2s ease;
  }

  .menu-item:hover {
    background: var(--bg-tertiary);
    color: var(--text-primary);
  }
  .menu-item:hover .menu-icon {
    opacity: 0.85;
  }

  .menu-item.active {
    background: rgba(0, 245, 255, 0.1);
    color: var(--neon-cyan);
    box-shadow: var(--glow-cyan);
  }
  .menu-item.active .menu-icon {
    opacity: 1;
    color: var(--neon-cyan);
  }

  .menu-label {
    flex: 1;
  }

  .settings-content {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 1.5rem;
    position: relative;
  }

  .pane[hidden] {
    display: none;
  }
</style>
