<script lang="ts">
  import Sidebar from '$lib/components/Sidebar.svelte';
  import TopBar from '$lib/components/TopBar.svelte';
  import StatusBar from '$lib/components/StatusBar.svelte';
  import InstallWizard from '$lib/components/InstallWizard.svelte';
  import { installComplete } from '$lib/stores/install';
  import { onMount } from 'svelte';

  let showInstallWizard = $state(false);
  let isReady = $state(false);

  onMount(() => {
    const unsubscribe = installComplete.subscribe((complete: boolean) => {
      showInstallWizard = !complete;
    });
    isReady = true;
    return unsubscribe;
  });
</script>

{#if isReady}
  {#if showInstallWizard}
    <InstallWizard />
  {:else}
    <div class="layout">
      <Sidebar />
      
      <main class="main-area">
        <TopBar />
        
        <div class="content">
          <slot />
        </div>
        
        <StatusBar />
      </main>
    </div>
  {/if}
{/if}

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
