<script lang="ts">
  let { id, label } = $props<{ id: string; label: string }>();

  // 品牌色 + 首字母降级;个别 agent 用专属 SVG 图形
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
  };

  const brand = $derived(
    BRAND[id] ?? { bg: 'rgba(94, 234, 212, 0.15)', fg: '#5eead4', initial: (label[0] ?? '?').toUpperCase() }
  );
</script>

<span class="agent-logo" style="background: {brand.bg}; color: {brand.fg};" aria-hidden="true">
  {#if id === 'claude-code'}
    <!-- Claude 星芒 -->
    <svg viewBox="0 0 24 24" fill="currentColor">
      <path d="M12 2l1.8 6.2L20 6.5l-4.4 4.6 5.6 2.9-6.3.7 1.7 6.1-4.6-4.4-4.6 4.4 1.7-6.1-6.3-.7 5.6-2.9L4 6.5l6.2 1.7z"/>
    </svg>
  {:else if id === 'node'}
    <!-- Node 六边形 -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
      <path d="M12 2.5l8.2 4.75v9.5L12 21.5l-8.2-4.75v-9.5z"/>
      <path d="M12 7v10M8.5 9l7 6" stroke-width="1.4"/>
    </svg>
  {:else if id === 'cursor-agent'}
    <!-- Cursor 指针 -->
    <svg viewBox="0 0 24 24" fill="currentColor">
      <path d="M5 3l14 8.5-6.2 1.4 3.4 6.3-2.6 1.4-3.4-6.3L6 19z"/>
    </svg>
  {:else if id === 'codex'}
    <!-- Codex 圆环 -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <circle cx="12" cy="12" r="8"/>
      <circle cx="12" cy="12" r="3.2" stroke-width="1.6"/>
    </svg>
  {:else if id === 'openclaw'}
    <!-- 爪痕 -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
      <path d="M6 4c2 4 2 12 0 16M12 3c2.5 4.5 2.5 13.5 0 18M18 4c2 4 2 12 0 16"/>
    </svg>
  {:else if id === 'hermes'}
    <!-- 翅膀 -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
      <path d="M3 15c4 0 6-2 7-5 1 3 3 5 7 5M5 19c4 0 9-1 12-6M12 10V4"/>
    </svg>
  {:else if id === 'opencode'}
    <!-- 终端 -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
      <rect x="3" y="4" width="18" height="16" rx="2"/>
      <path d="M7 9l3 3-3 3M13 15h4"/>
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
  .initial {
    font-size: 1.25rem;
    font-weight: 700;
    line-height: 1;
  }
</style>
