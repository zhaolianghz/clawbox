<script lang="ts">
  import { _, locale } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import { themeChoice, setTheme, type ThemeChoice } from '$lib/theme';
  import ProvidersPage from '../../routes/providers/+page.svelte';
  import McpPage from '../../routes/mcp/+page.svelte';
  import AgentHubPage from '../../routes/agents/+page.svelte';
  import CapabilitiesPage from '../../routes/capabilities/+page.svelte';
  import AboutPage from '../../routes/about/+page.svelte';
  import UsagePage from '../../routes/usage/+page.svelte';

  // v1 定位:AI agent 统一配置中心。主导航自上而下:
  // 基础(服务商) → 能力三件套(MCP/技能/记忆,工具→行为→知识) → 消费者(Agent 管理)。
  // skills 与 memory 复用同一个 CapabilitiesPage 实例(keep-alive),由 tab prop 锁定。
  type SectionId = 'providers' | 'mcp' | 'skills' | 'memory' | 'agents' | 'usage' | 'about';

  interface Props {
    /** standalone:作为应用本体渲染(无返回按钮,标题为 ClawBox,追加「关于」节) */
    standalone?: boolean;
    onexit?: () => void;
  }
  let { standalone = false, onexit }: Props = $props();

  // 「关于」不在主导航(挪到侧边栏页脚),sections 不再依赖 standalone。
  const sections: { id: SectionId; labelKey: string }[] = [
    { id: 'providers', labelKey: 'nav.providers' },
    { id: 'mcp', labelKey: 'nav.mcp' },
    { id: 'skills', labelKey: 'nav.skills' },
    { id: 'memory', labelKey: 'nav.memory' },
    { id: 'agents', labelKey: 'nav.agents' },
    { id: 'usage', labelKey: 'nav.usage' },
  ];

  /** 语言切换:svelte-i18n locale + localStorage 持久化(i18n/index.ts 启动时读取) */
  function setLang(l: 'en' | 'zh') {
    locale.set(l);
    try {
      localStorage.setItem('clawbox.locale', l);
    } catch { /* 隐私模式等存储不可用时仅本次生效 */ }
  }

  let activeSection = $state<SectionId>('providers');
  // keep-alive:子页首次访问后常驻,切回秒开,避免每次重跑 CLI 探测
  let mounted = $state<Record<SectionId, boolean>>({
    providers: true,
    mcp: false,
    skills: false,
    memory: false,
    agents: false,
    usage: false,
    about: false,
  });

  function select(id: SectionId) {
    activeSection = id;
    mounted[id] = true;
  }

  onMount(() => {
    // 预热:首屏就绪后静默挂载其余子页,数据提前加载
    setTimeout(() => {
      mounted.mcp = true;
      mounted.skills = true;
      mounted.memory = true;
      mounted.agents = true;
      mounted.usage = true;
      if (standalone) mounted.about = true;
    }, 600);
  });

  /** skills/memory 是否任一可见(共用 CapabilitiesPage 单实例) */
  function capabilitiesVisible(s: SectionId): boolean {
    return s === 'skills' || s === 'memory';
  }
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
          {:else if s.id === 'mcp'}
            <!-- MCP:芯片/工具服务 -->
            <svg class="menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <rect x="4" y="4" width="16" height="16" rx="2"/>
              <rect x="9" y="9" width="6" height="6"/>
              <line x1="9" y1="1" x2="9" y2="4"/>
              <line x1="15" y1="1" x2="15" y2="4"/>
              <line x1="9" y1="20" x2="9" y2="23"/>
              <line x1="15" y1="20" x2="15" y2="23"/>
              <line x1="20" y1="9" x2="23" y2="9"/>
              <line x1="20" y1="14" x2="23" y2="14"/>
              <line x1="1" y1="9" x2="4" y2="9"/>
              <line x1="1" y1="14" x2="4" y2="14"/>
            </svg>
          {:else if s.id === 'skills'}
            <!-- 技能:闪电(可复用行为) -->
            <svg class="menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>
            </svg>
          {:else if s.id === 'memory'}
            <!-- 记忆:数据库(持久知识) -->
            <svg class="menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <ellipse cx="12" cy="5" rx="9" ry="3"/>
              <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"/>
              <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"/>
            </svg>
          {:else if s.id === 'agents'}
            <svg class="menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
              <circle cx="8.5" cy="8.5" r="1.5"/>
              <polyline points="21 15 16 10 5 21"/>
            </svg>
          {:else if s.id === 'usage'}
            <!-- 用量:柱状图 -->
            <svg class="menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <line x1="12" y1="20" x2="12" y2="10"/>
              <line x1="18" y1="20" x2="18" y2="4"/>
              <line x1="6" y1="20" x2="6" y2="14"/>
              <line x1="3" y1="20" x2="21" y2="20"/>
            </svg>
          {/if}
          <span class="menu-label">{$_(s.labelKey)}</span>
        </button>
      {/each}
    </nav>

    <!-- 页脚:关于入口(仅 standalone)+ 语言切换器 -->
    <div class="menu-footer">
      {#if standalone}
        <button
          class="menu-item"
          class:active={activeSection === 'about'}
          onclick={() => select('about')}
        >
          <svg class="menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10"/>
            <line x1="12" y1="16" x2="12" y2="12"/>
            <line x1="12" y1="8" x2="12.01" y2="8"/>
          </svg>
          <span class="menu-label">{$_('nav.about')}</span>
        </button>
      {/if}
      <div class="lang-switch" role="group" aria-label={$_('nav.language')} title={$_('nav.language')}>
        <button
          class="lang-btn"
          class:active={!$locale?.startsWith('zh')}
          onclick={() => setLang('en')}
        >EN</button>
        <button
          class="lang-btn"
          class:active={!!$locale?.startsWith('zh')}
          onclick={() => setLang('zh')}
        >中文</button>
      </div>
      <div class="lang-switch theme-switch" role="group" aria-label={$_('theme.label')} title={$_('theme.label')}>
        {#each [
          { id: 'system', label: $_('theme.system'), icon: 'M4 5h16v11H4zM8 20h8M12 16v4' },
          { id: 'light', label: $_('theme.light'), icon: 'M12 3v2M12 19v2M5 5l1.5 1.5M17.5 17.5L19 19M3 12h2M19 12h2M5 19l1.5-1.5M17.5 6.5L19 5' },
          { id: 'dark', label: $_('theme.dark'), icon: 'M21 12.8A8.5 8.5 0 1 1 11.2 3a6.6 6.6 0 0 0 9.8 9.8z' },
        ] as opt (opt.id)}
          <button
            class="lang-btn theme-btn"
            class:active={$themeChoice === opt.id}
            onclick={() => setTheme(opt.id as ThemeChoice)}
            title={opt.label}
            aria-label={opt.label}
          >
            {#if opt.id === 'light'}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <circle cx="12" cy="12" r="4" />
                <path d={opt.icon} />
              </svg>
            {:else}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d={opt.icon} />
              </svg>
            {/if}
          </button>
        {/each}
      </div>
      <!-- 风格主题选择器已移至「关于」页面 -->
    </div>
  </aside>

  <div class="settings-content">
    {#if mounted.providers}
      <div class="pane" hidden={activeSection !== 'providers'}><ProvidersPage /></div>
    {/if}
    {#if mounted.mcp}
      <div class="pane" hidden={activeSection !== 'mcp'}><McpPage /></div>
    {/if}
    {#if mounted.skills || mounted.memory}
      <div class="pane" hidden={!capabilitiesVisible(activeSection)}>
        <CapabilitiesPage tab={activeSection === 'memory' ? 'memory' : 'skills'} />
      </div>
    {/if}
    {#if mounted.agents}
      <div class="pane" hidden={activeSection !== 'agents'}><AgentHubPage /></div>
    {/if}
    {#if mounted.usage}
      <div class="pane" hidden={activeSection !== 'usage'}><UsagePage /></div>
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
    border: 1px solid var(--border-subtle);
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
    border-right: 1px solid var(--border-subtle);
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
    border-bottom: 1px solid var(--border-subtle);
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

  /* 页脚:贴底,细分隔线 */
  .menu-footer {
    margin-top: auto;
    padding: 0.6rem 0.5rem 0.4rem;
    border-top: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .lang-switch {
    display: flex;
    gap: 0.25rem;
    padding: 0 0.35rem;
  }
  .lang-btn {
    flex: 1;
    padding: 0.3rem 0;
    font-size: 0.72rem;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    cursor: pointer;
    transition: all 0.2s ease;
  }
  .lang-btn:hover {
    color: var(--text-primary);
    background: var(--bg-tertiary);
  }
  .lang-btn.active {
    background: color-mix(in srgb, var(--neon-cyan) 12%, transparent);
    border-color: var(--neon-cyan);
    color: var(--neon-cyan);
  }
  .theme-switch { margin-top: 0.35rem; }
  .theme-btn { display: inline-flex; align-items: center; justify-content: center; padding: 0.32rem 0; }
  .theme-btn svg { width: 15px; height: 15px; }
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
