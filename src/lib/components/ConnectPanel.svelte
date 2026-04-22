<script lang="ts">
  import { useSettings, updateSettings } from "$lib/stores/settings.svelte";
  import { useConnectionHistory, addToHistory, removeFromHistory } from "$lib/stores/connection-history.svelte";
  import { generateConnectionId } from "$lib/utils/codec";
  
  let mode = $state<"local" | "host" | "client">("local");
  let partnerId = $state("127.0.0.1");
  let serverPort = $state(4567);
  let connecting = $state(false);
  let errorMsg = $state("");
  
  const settings = useSettings();
  const history = useConnectionHistory();
  
  async function handleConnect() {
    if (mode === "client" && !partnerId.trim()) {
      errorMsg = "Please enter a Partner ID";
      return;
    }
    connecting = true;
    errorMsg = "";
    try {
      const [host, port] = partnerId.includes(":")
        ? partnerId.split(":")
        : [partnerId, "4567"];
      // TODO: Connect using session store
    } catch (e: any) {
      errorMsg = e.toString();
    } finally {
      connecting = false;
    }
  }
  
  async function handleHost() {
    connecting = true;
    errorMsg = "";
    try {
      // TODO: Start server using session store
    } catch (e: any) {
      errorMsg = e.toString();
    } finally {
      connecting = false;
    }
  }
  
  function connectToHistoryItem(item: any) {
    partnerId = item.address;
    mode = "client";
  }
</script>

<div class="connect-panel">
  <div class="logo">
    <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
      <rect width="48" height="48" rx="12" fill="var(--vr-accent)"/>
      <path d="M14 18h20M14 24h20M14 30h12" stroke="white" stroke-width="2" stroke-linecap="round"/>
    </svg>
    <h1>VibeRemote</h1>
  </div>
  
  <div class="vr-card">
    <div class="mode-selector">
      <button class:active={mode === "local"} onclick={() => mode = "local"}>
        Local
      </button>
      <button class:active={mode === "host"} onclick={() => mode = "host"}>
        Host
      </button>
      <button class:active={mode === "client"} onclick={() => mode = "client"}>
        Connect
      </button>
    </div>
    
    {#if mode === "client"}
      <div class="input-group">
        <label for="partner-id">Remote Address</label>
        <input
          id="partner-id"
          class="vr-input"
          type="text"
          placeholder="Enter server IP (e.g., 192.168.1.100)"
          bind:value={partnerId}
          onkeydown={(e) => e.key === "Enter" && handleConnect()}
        />
      </div>
    {:else if mode === "host"}
      <div class="input-group">
        <label for="server-port">Server Port</label>
        <input
          id="server-port"
          class="vr-input"
          type="number"
          placeholder="4567"
          bind:value={serverPort}
        />
      </div>
    {/if}
    
    <div class="button-row">
      {#if mode === "local"}
        <button class="vr-button vr-button-primary">
          Start Capture
        </button>
      {:else}
        <button
          class="vr-button vr-button-primary"
          onclick={mode === "host" ? handleHost : handleConnect}
          disabled={connecting}
        >
          {#if connecting}
            <span class="vr-spinner"></span>
            {mode === "host" ? "Starting..." : "Connecting..."}
          {:else}
            {mode === "host" ? "Start Server" : "Connect"}
          {/if}
        </button>
      {/if}
    </div>
    
    {#if errorMsg}
      <div class="error-message">{errorMsg}</div>
    {/if}
  </div>
  
  {#if history.length > 0}
    <div class="history-section">
      <h3>Recent Connections</h3>
      <div class="history-grid">
        {#each history as item}
          <button class="history-card" onclick={() => connectToHistoryItem(item)}>
            <span class="icon">{item.icon || "💻"}</span>
            <span class="alias">{item.alias}</span>
            <span class="address">{item.address}</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .connect-panel {
    max-width: 480px;
    margin: 0 auto;
    text-align: center;
  }
  
  .logo {
    margin-bottom: 2rem;
  }
  
  .logo h1 {
    font-size: 2.5rem;
    font-weight: 700;
    margin: 0.5rem 0 0.25rem;
    background: linear-gradient(135deg, var(--vr-text-primary), var(--vr-accent));
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
  }
  
  .vr-card {
    padding: 2rem;
  }
  
  .mode-selector {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1.5rem;
    background: rgba(15, 23, 42, 0.4);
    padding: 0.5rem;
    border-radius: var(--vr-radius-md);
  }
  
  .mode-selector button {
    flex: 1;
    padding: 0.75rem;
    font-size: 0.875rem;
    font-weight: 600;
    background: transparent;
    border: none;
    border-radius: var(--vr-radius-sm);
    color: var(--vr-text-secondary);
    cursor: pointer;
    transition: all 0.2s;
  }
  
  .mode-selector button.active {
    background: var(--vr-accent);
    color: white;
  }
  
  .input-group {
    text-align: left;
    margin-bottom: 1.5rem;
  }
  
  .input-group label {
    display: block;
    font-size: 0.875rem;
    font-weight: 500;
    margin-bottom: 0.5rem;
    color: var(--vr-text-secondary);
  }
  
  .button-row {
    display: flex;
    gap: 0.75rem;
  }
  
  .vr-button-primary {
    flex: 1;
  }
  
  .error-message {
    margin-top: 1rem;
    padding: 0.75rem;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: var(--vr-radius-sm);
    color: var(--vr-danger);
    font-size: 0.875rem;
  }
  
  .history-section {
    margin-top: 2rem;
    text-align: left;
  }
  
  .history-section h3 {
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--vr-text-secondary);
    margin-bottom: 1rem;
  }
  
  .history-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 0.75rem;
  }
  
  .history-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.25rem;
    padding: 1rem;
    background: var(--vr-bg-elevated);
    border: 1px solid var(--vr-border);
    border-radius: var(--vr-radius-md);
    cursor: pointer;
    transition: all 0.2s;
  }
  
  .history-card:hover {
    border-color: var(--vr-accent);
  }
  
  .icon {
    font-size: 1.5rem;
  }
  
  .alias {
    font-weight: 600;
    color: var(--vr-text-primary);
  }
  
  .address {
    font-size: 0.75rem;
    color: var(--vr-text-tertiary);
  }
</style>