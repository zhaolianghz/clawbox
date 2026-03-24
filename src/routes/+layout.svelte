<script lang="ts">
  import Sidebar from '$lib/components/Sidebar.svelte';
  import TopBar from '$lib/components/TopBar.svelte';
  import StatusBar from '$lib/components/StatusBar.svelte';
</script>

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
