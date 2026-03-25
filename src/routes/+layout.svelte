<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { goto } from '$app/navigation';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import TopBar from '$lib/components/TopBar.svelte';
  import StatusBar from '$lib/components/StatusBar.svelte';

  let activeItem = $state('home');
  let gatewayStatus = $state('stopped');
  let gatewayVersion = $state('v1.0.0');
  let tokenCount = $state(0);

  function handleNavigate(event: CustomEvent<string>) {
    activeItem = event.detail;
    goto(event.detail === 'home' ? '/' : `/${event.detail}`);
  }

  onMount(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const status = await invoke<{ running: boolean; version: string }>('get_gateway_status');
      gatewayStatus = status.running ? 'running' : 'stopped';
      gatewayVersion = status.version;
    } catch {
      gatewayStatus = 'stopped';
    }
  });
</script>

<div class="layout">
  <Sidebar {activeItem} onnavigate={handleNavigate} />
  
  <div class="main-area">
    <TopBar />
    <div class="content">
      <slot />
    </div>
    <StatusBar {gatewayStatus} {gatewayVersion} {tokenCount} />
  </div>
</div>

<style>
  .layout {
    display: flex;
    height: 100vh;
    background: var(--bg-primary);
    overflow: hidden;
  }
  
  .main-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  
  .content {
    flex: 1;
    overflow: auto;
    padding: 1.5rem;
  }
</style>
