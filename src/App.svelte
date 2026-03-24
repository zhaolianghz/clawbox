<script lang="ts">
  let greetMsg = $state("");
  let name = $state("");

  async function greet() {
    greetMsg = `Hello, ${name}! Welcome to ClawBox.`;
  }
</script>

<main class="flex flex-col items-center justify-center min-h-screen bg-gradient-to-br from-gray-900 to-gray-800">
  <h1 class="text-4xl font-bold text-neon-cyan mb-8">Welcome to ClawBox</h1>
  
  <div class="flex gap-4 mb-8">
    <input
      type="text"
      bind:value={name}
      placeholder="Enter your name..."
      class="px-4 py-2 bg-gray-800 border border-neon-purple rounded-lg text-white placeholder-gray-500 focus:outline-none focus:border-neon-cyan"
    />
    <button
      onclick={greet}
      class="px-6 py-2 bg-neon-purple text-white font-semibold rounded-lg hover:bg-neon-pink transition-colors"
    >
      Greet
    </button>
  </div>

  {#if greetMsg}
    <p class="text-xl text-neon-green">{greetMsg}</p>
  {/if}

  <p class="mt-12 text-gray-400">
    Edit <code class="bg-gray-800 px-2 py-1 rounded text-neon-cyan">src/App.svelte</code> to get started.
  </p>
</main>
