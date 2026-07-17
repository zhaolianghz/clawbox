<script lang="ts">
  // v1 定位:AI agent 统一配置中心。应用本体即原「设置」界面(拉平,无覆盖层):
  // 服务商 / 配置 / Agent 管理 / 能力 / 关于,由 SettingsLayout standalone 模式承载。
  // i18n 初始化在 main.ts 里 import './lib/i18n/index.ts' 完成,组件渲染前字典已就绪。
  import SettingsLayout from './lib/components/SettingsLayout.svelte';
</script>

<div class="app">
  <SettingsLayout standalone />
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
    height: 100vh;
    background-color: #0a0a0f;
    overflow: hidden;
  }
</style>
