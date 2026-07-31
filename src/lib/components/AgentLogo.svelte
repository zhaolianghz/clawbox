<script lang="ts">
  import { AGENT_ICONS } from '$lib/data/providerIcons';

  let { id, label } = $props<{ id: string; label: string }>();

  // 品牌色 + 首字母降级;命中 lobe-icons 的 agent 用真实 logo
  const BRAND: Record<string, { bg: string; fg: string; initial: string }> = {
    node: { bg: 'rgba(67, 133, 61, 0.18)', fg: '#8cc84b', initial: 'N' },
    'claude-code': { bg: 'rgba(217, 119, 87, 0.18)', fg: '#d97757', initial: 'C' },
    codex: { bg: 'rgba(116, 170, 156, 0.18)', fg: '#74aa9c', initial: 'C' },
    openclaw: { bg: 'rgba(224, 83, 61, 0.18)', fg: '#e0533d', initial: 'O' },
    opencode: { bg: 'rgba(148, 163, 184, 0.18)', fg: '#e2e8f0', initial: 'O' },
    codebuddy: { bg: 'rgba(0, 82, 217, 0.22)', fg: '#4d8bff', initial: 'B' },
    'cursor-agent': { bg: 'rgba(226, 232, 240, 0.14)', fg: '#e2e8f0', initial: 'C' },
    kimi: { bg: 'rgba(107, 87, 255, 0.2)', fg: '#8f7bff', initial: 'K' },
    qodercli: { bg: 'rgba(94, 92, 230, 0.2)', fg: '#7d7bff', initial: 'Q' },
    hermes: { bg: 'rgba(212, 160, 23, 0.18)', fg: '#d4a017', initial: 'H' },
    gemini: { bg: 'rgba(66, 133, 244, 0.18)', fg: '#8ab4f8', initial: 'G' },
    cline: { bg: 'rgba(158, 134, 255, 0.18)', fg: '#b3a1ff', initial: 'C' },
    pi: { bg: 'rgba(251, 191, 36, 0.16)', fg: '#fbbf24', initial: 'π' },
    'qwen-code': { bg: 'rgba(97, 92, 237, 0.2)', fg: '#8b87ff', initial: 'Q' },
    'copilot-cli': { bg: 'rgba(139, 148, 158, 0.18)', fg: '#c9d1d9', initial: 'C' },
    'trae-agent': { bg: 'rgba(240, 68, 86, 0.18)', fg: '#f87171', initial: 'T' },
  };

  const brand = $derived(
    BRAND[id] ?? { bg: 'rgba(94, 234, 212, 0.15)', fg: '#5eead4', initial: (label[0] ?? '?').toUpperCase() }
  );

  // lobe-icons 真实品牌 logo(构建期内联的静态资产,{@html} 内容不含用户输入,
  // 无注入风险);单色版用 currentColor,外层品牌色上色。
  const icon = $derived(AGENT_ICONS[id]);
</script>

<span class="agent-logo" style="background: {brand.bg}; color: {brand.fg};" aria-hidden="true">
  {#if icon}
    <span class="brand-icon">{@html icon}</span>
  {:else if id === 'node'}
    <!-- Node 六边形 -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
      <path d="M12 2.5l8.2 4.75v9.5L12 21.5l-8.2-4.75v-9.5z"/>
      <path d="M12 7v10M8.5 9l7 6" stroke-width="1.4"/>
    </svg>
  {:else if id === 'hermes'}
    <!-- 翅膀 -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
      <path d="M3 15c4 0 6-2 7-5 1 3 3 5 7 5M5 19c4 0 9-1 12-6M12 10V4"/>
    </svg>
  {:else if id === 'gemini'}
    <!-- 四角星火花 -->
    <svg viewBox="0 0 24 24" fill="currentColor" stroke="none">
      <path d="M12 2c.9 5.2 4.8 9.1 10 10-5.2.9-9.1 4.8-10 10-.9-5.2-4.8-9.1-10-10 5.2-.9 9.1-4.8 10-10z"/>
    </svg>
  {:else if id === 'copilot-cli'}
    <!-- 护目镜 -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
      <rect x="2.5" y="8.5" width="7.5" height="7" rx="2.5"/>
      <rect x="14" y="8.5" width="7.5" height="7" rx="2.5"/>
      <path d="M10 12h4" stroke-linecap="round"/>
    </svg>
  {:else}
    <span class="initial">{brand.initial}</span>
  {/if}
</span>

<style>
  .agent-logo {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border: 1px solid rgba(255, 255, 255, 0.08);
  }
  .agent-logo svg {
    width: 24px;
    height: 24px;
  }
  .brand-icon {
    width: 24px;
    height: 24px;
    display: inline-flex;
  }
  .brand-icon :global(svg) {
    width: 100%;
    height: 100%;
  }
  .initial {
    font-size: 1.25rem;
    font-weight: 700;
    line-height: 1;
  }
</style>
