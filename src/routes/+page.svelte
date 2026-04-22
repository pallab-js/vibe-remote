<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { fade, slide } from "svelte/transition";
  import "tailwindcss";
  import "$lib/design-system.css";

  // State
  let connectionMode = $state<"server" | "client" | "local">("local");
  let partnerId = $state("127.0.0.1");
  let serverPort = $state(4567);
  let isConnected = $state(false);
  let isCapturing = $state(false);
  let connectionStatus = $state<"disconnected" | "connecting" | "connected" | "error" | "server">("disconnected");
  let fps = $state(0);
  let latency = $state(0);
  let errorMsg = $state("");
  let showToolbar = $state(true);
  let statusMessage = $state("Ready to connect");
  let selectedDisplay = $state(0);
  let availableDisplays = $state<Array<[string, number, number]>>([]);
  let isDarkMode = $state(true);
  
  // Onboarding state
  let hasCompletedOnboarding = $state(false);
  let onboardingStep = $state(1);
  
  // Security: Consent and permission state
  let allowRemoteInput = $state(false); // CRIT-2: Remote input disabled by default
  let allowClipboardSync = $state(false); // CRIT-3: Clipboard sync disabled by default
  let showInputConsent = $state(false); // Show consent modal
  let showClipboardConsent = $state(false); // Show clipboard consent
  let viewOnlyMode = $state(true); // Default to view-only for safety
  let inputConsentGiven = $state(false); // User explicitly consented to input
  let clipboardConsentGiven = $state(false); // User explicitly consented to clipboard

  // Frame tracking
  let frameCount = 0;
  let lastFpsUpdate = Date.now();
  // svelte-ignore non_reactive_update
  let canvasRef: HTMLCanvasElement;
  let toolbarTimeout: ReturnType<typeof setTimeout>;

  onMount(() => {
    (async () => {
    // Detect system dark mode preference
    isDarkMode = window.matchMedia('(prefers-color-scheme: dark)').matches;
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
      isDarkMode = e.matches;
    });

    // Check onboarding state from localStorage
    const savedOnboarding = localStorage.getItem('vibe-remote-onboarding');
    hasCompletedOnboarding = savedOnboarding === 'complete';

    // Initialize VibeRemote
    try {
      await invoke("init_vibe");
      console.log("VibeRemote initialized");
      statusMessage = "Initialized - Ready to capture";
      
      // Get available displays
      try {
        availableDisplays = await invoke("get_displays");
        console.log("Available displays:", availableDisplays);
      } catch (err) {
        console.warn("Could not fetch displays:", err);
      }
    } catch (err) {
      console.error("Failed to initialize:", err);
      errorMsg = "Failed to initialize - check accessibility permissions";
      // Show onboarding if init fails (likely permission issue)
      if (!hasCompletedOnboarding) {
        hasCompletedOnboarding = false;
        onboardingStep = 1;
      }
    }

    // Listen for frame events
    const unlistenFrame = await listen("frame", (event: any) => {
      if (canvasRef && event.payload) {
        renderFrame(event.payload);
        frameCount++;
        
        // Update FPS every second
        const now = Date.now();
        if (now - lastFpsUpdate >= 1000) {
          fps = frameCount;
          frameCount = 0;
          lastFpsUpdate = now;
        }
      }
    });

    // Auto-hide toolbar
    resetToolbarTimeout();

    cleanup = () => {
      unlistenFrame();
      clearTimeout(toolbarTimeout);
    };
    })();

    let cleanup: (() => void) | undefined;
    return () => {
      cleanup?.();
    };
  });

  function saveOnboardingState() {
    localStorage.setItem('vibe-remote-onboarding', 'complete');
    hasCompletedOnboarding = true;
  }

  function resetToolbarTimeout() {
    clearTimeout(toolbarTimeout);
    showToolbar = true;
    toolbarTimeout = setTimeout(() => {
      if (isCapturing) {
        showToolbar = false;
      }
    }, 3000);
  }

  // Render frame to canvas
  function renderFrame(frameData: any) {
    if (!canvasRef || !frameData.data_b64) return;
    
    const ctx = canvasRef.getContext("2d");
    if (!ctx) return;

    // Decode base64 to raw pixels
    const binaryStr = atob(frameData.data_b64);
    const bytes = new Uint8Array(binaryStr.length);
    for (let i = 0; i < binaryStr.length; i++) {
      bytes[i] = binaryStr.charCodeAt(i);
    }
    
    // Create ImageData from RGBA buffer (data is already RGBA)
    const imageData = new ImageData(frameData.width, frameData.height);
    const data = bytes;
    
    // Data is already RGBA from backend - copy directly
    for (let i = 0; i < data.length; i += 4) {
      imageData.data[i] = data[i];     // R
      imageData.data[i + 1] = data[i + 1]; // G
      imageData.data[i + 2] = data[i + 2]; // B
      imageData.data[i + 3] = data[i + 3]; // A
    }

    canvasRef.width = frameData.width;
    canvasRef.height = frameData.height;
    ctx.putImageData(imageData, 0, 0);
    
    // Calculate actual round-trip latency (receive time - capture time)
    const receiveTime = Date.now();
    const captureTime = Number(frameData.timestamp);
    latency = Math.max(0, receiveTime - captureTime);
  }

  // Start capture
  async function startCapture() {
    try {
      errorMsg = "";
      statusMessage = "Starting capture...";
      await invoke("start_capture", { displayIndex: selectedDisplay });
      isCapturing = true;
      connectionStatus = "connected";
      statusMessage = `Capturing display ${selectedDisplay + 1}`;
    } catch (err: any) {
      errorMsg = err.toString();
      connectionStatus = "error";
      statusMessage = "Capture failed";
    }
  }

  // Stop capture
  async function stopCapture() {
    try {
      await invoke("stop_capture");
      isCapturing = false;
      isConnected = false;
      connectionStatus = "disconnected";
      statusMessage = "Disconnected";
    } catch (err) {
      console.error("Failed to stop capture:", err);
    }
  }

  // Connect to remote
  // SEC-1: Now supports TOFU mode (no fingerprint required)
  async function connect() {
    if (!partnerId.trim()) {
      errorMsg = "Please enter a Partner ID";
      return;
    }

    try {
      errorMsg = "";
      connectionStatus = "connecting";
      statusMessage = "Connecting...";

      // Parse host:port or use defaults
      const [host, port] = partnerId.includes(":")
        ? partnerId.split(":")
        : [partnerId, "4567"];

      // SEC-1: Connect without fingerprint (TOFU mode)
      // Users can optionally provide a fingerprint for strict pinning
      await invoke("connect_remote", {
        params: {
          host,
          port: parseInt(port),
          serverFingerprint: null  // TOFU mode - accepts first cert
        }
      });

      isConnected = true;
      connectionStatus = "connected";
      statusMessage = `Connected to ${partnerId}`;
    } catch (err: any) {
      errorMsg = err.toString();
      connectionStatus = "error";
      statusMessage = "Connection failed";
    }
  }

  // Start server mode (host)
  async function startServer() {
    try {
      errorMsg = "";
      connectionStatus = "connecting";
      statusMessage = `Starting server on port ${serverPort}...`;
      
      // Start QUIC server
      await invoke("start_server", { port: serverPort });
      
      // Start capturing and streaming to clients
      statusMessage = "Starting remote stream...";
      await invoke("start_remote_stream", { displayIndex: selectedDisplay });
      
      connectionStatus = "server";
      isConnected = true;
      isCapturing = true;
      statusMessage = `Hosting on port ${serverPort} - waiting for clients...`;
    } catch (err: any) {
      errorMsg = err.toString();
      connectionStatus = "error";
      statusMessage = "Server start failed";
    }
  }

  // Mouse handling - SECURITY: Block if remote input not consented
  async function handleMouseMove(event: MouseEvent) {
    if (!isCapturing || !canvasRef || viewOnlyMode || !inputConsentGiven) return;
    
    const rect = canvasRef.getBoundingClientRect();
    const scaleX = canvasRef.width / rect.width;
    const scaleY = canvasRef.height / rect.height;
    
    const x = Math.floor((event.clientX - rect.left) * scaleX);
    const y = Math.floor((event.clientY - rect.top) * scaleY);
    
    try {
      await invoke("send_mouse_input", {
        eventType: "move",
        x,
        y
      });
    } catch (err) {
      // Silently fail for mouse move
    }
  }

  async function handleMouseDown(event: MouseEvent) {
    if (!isCapturing || !canvasRef || viewOnlyMode || !inputConsentGiven) return;
    
    const button = event.button === 2 ? "right" : "left";
    
    const rect = canvasRef.getBoundingClientRect();
    const scaleX = canvasRef.width / rect.width;
    const scaleY = canvasRef.height / rect.height;
    
    const x = Math.floor((event.clientX - rect.left) * scaleX);
    const y = Math.floor((event.clientY - rect.top) * scaleY);
    
    try {
      await invoke("send_mouse_input", {
        eventType: "down",
        x,
        y,
        button
      });
    } catch (err) {
      console.error("Failed to send mouse down:", err);
    }
  }

  async function handleMouseUp(event: MouseEvent) {
    if (!isCapturing || !canvasRef || viewOnlyMode || !inputConsentGiven) return;
    
    const button = event.button === 2 ? "right" : "left";
    
    const rect = canvasRef.getBoundingClientRect();
    const scaleX = canvasRef.width / rect.width;
    const scaleY = canvasRef.height / rect.height;
    
    const x = Math.floor((event.clientX - rect.left) * scaleX);
    const y = Math.floor((event.clientY - rect.top) * scaleY);
    
    try {
      await invoke("send_mouse_input", {
        eventType: "up",
        x,
        y,
        button
      });
    } catch (err) {
      console.error("Failed to send mouse up:", err);
    }
  }

  async function handleWheel(event: WheelEvent) {
    if (!isCapturing || !canvasRef || viewOnlyMode || !inputConsentGiven) return;
    event.preventDefault();
    
    try {
      await invoke("send_mouse_input", {
        eventType: "wheel",
        x: 0,
        y: -Math.sign(event.deltaY) * 10
      });
    } catch (err) {
      console.error("Failed to send wheel:", err);
    }
  }

  // Keyboard handling - SECURITY: Block if remote input not consented
  async function handleKeyDown(event: KeyboardEvent) {
    if (!isCapturing || viewOnlyMode || !inputConsentGiven) return;
    
    // Prevent default for most keys when capturing
    event.preventDefault();
    event.stopPropagation();
    
    try {
      await invoke("send_keyboard_input", {
        key: event.key,
        eventType: "down"
      });
    } catch (err) {
      console.error("Failed to send key down:", err);
    }
  }

  async function handleKeyUp(event: KeyboardEvent) {
    if (!isCapturing || viewOnlyMode || !inputConsentGiven) return;
    
    event.preventDefault();
    event.stopPropagation();
    
    try {
      await invoke("send_keyboard_input", {
        key: event.key,
        eventType: "up"
      });
    } catch (err) {
      console.error("Failed to send key up:", err);
    }
  }

  // Prevent context menu on canvas
  function preventContextMenu(event: Event) {
    event.preventDefault();
  }

  // Cleanup
  onDestroy(() => {
    clearTimeout(toolbarTimeout);
  });

  // CRIT-2: Consent handlers - now enforce backend consent too
  async function grantInputConsent() {
    inputConsentGiven = true;
    viewOnlyMode = false;
    showInputConsent = false;
    statusMessage = "Remote control enabled";
    // SEC-3: Enforce consent at backend level
    try {
      await invoke("grant_input_consent");
    } catch (err) {
      console.error("Failed to grant backend input consent:", err);
    }
  }

  async function revokeInputConsent() {
    inputConsentGiven = false;
    viewOnlyMode = true;
    statusMessage = "View-only mode";
    // SEC-3: Revoke backend consent
    try {
      await invoke("revoke_input_consent");
    } catch (err) {
      console.error("Failed to revoke backend input consent:", err);
    }
  }

  // CRIT-3: Clipboard consent handlers - now enforce backend consent too
  async function grantClipboardConsent() {
    clipboardConsentGiven = true;
    showClipboardConsent = false;
    statusMessage = "Clipboard sync enabled";
    // SEC-3: Enforce consent at backend level
    try {
      await invoke("grant_clipboard_consent");
    } catch (err) {
      console.error("Failed to grant backend clipboard consent:", err);
    }
  }

  async function revokeClipboardConsent() {
    clipboardConsentGiven = false;
    statusMessage = "Clipboard sync disabled";
    // SEC-3: Revoke backend consent
    try {
      await invoke("revoke_clipboard_consent");
    } catch (err) {
      console.error("Failed to revoke backend clipboard consent:", err);
    }
  }
</script>

<svelte:window
  onkeydown={handleKeyDown}
  onkeyup={handleKeyUp}
/>

<main class="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-800 text-slate-100 flex items-center justify-center p-8 font-[Inter]">
  <!-- Connection Dashboard -->
  {#if !isCapturing}
    <div class="max-w-lg w-full text-center" transition:fade>
      <div class="mb-10">
        <div class="mb-4">
          <svg width="48" height="48" viewBox="0 0 48 48" fill="none" class="mx-auto">
            <rect width="48" height="48" rx="12" fill="#3b82f6"/>
            <path d="M14 18h20M14 24h20M14 30h12" stroke="white" stroke-width="2" stroke-linecap="round"/>
          </svg>
        </div>
        <h1 class="text-5xl font-bold my-2 bg-gradient-to-r from-slate-100 to-blue-500 bg-clip-text text-transparent">VibeRemote</h1>
        <p class="text-slate-400 text-base m-0">Modern Remote Desktop</p>
      </div>

      <div class="bg-slate-800/60 backdrop-blur-xl border border-slate-700/20 rounded-3xl p-8 shadow-2xl">
        <!-- Mode Selector -->
        <div class="mode-selector">
          <button
            class:active={connectionMode === "local"}
            onclick={() => connectionMode = "local"}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
              <rect x="2" y="3" width="20" height="14" rx="2" stroke="currentColor" stroke-width="2"/>
              <path d="M8 21h8M12 17v4" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
            </svg>
            Local
          </button>
          <button
            class:active={connectionMode === "server"}
            onclick={() => connectionMode = "server"}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
              <rect x="2" y="2" width="20" height="8" rx="2" stroke="currentColor" stroke-width="2"/>
              <rect x="2" y="14" width="20" height="8" rx="2" stroke="currentColor" stroke-width="2"/>
              <circle cx="6" cy="6" r="1" fill="currentColor"/>
              <circle cx="6" cy="18" r="1" fill="currentColor"/>
            </svg>
            Host
          </button>
          <button
            class:active={connectionMode === "client"}
            onclick={() => connectionMode = "client"}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
              <path d="M5 12h14M12 5l7 7-7 7" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            Connect
          </button>
        </div>
        <!-- Connection Inputs -->
        {#if connectionMode === "client"}
          <div class="input-group">
            <label for="partner-id">Remote Address</label>
            <input
              id="partner-id"
              type="text"
              placeholder="Enter server IP (e.g., 192.168.1.100)"
              bind:value={partnerId}
              onkeydown={(e) => e.key === "Enter" && connect()}
            />
          </div>
        {:else if connectionMode === "server"}
          <div class="input-group">
            <label for="server-port">Server Port</label>
            <input
              id="server-port"
              type="number"
              placeholder="4567"
              bind:value={serverPort}
              min="1024"
              max="65535"
            />
          </div>
        {/if}

        <div class="button-row">
          {#if connectionMode === "local"}
            <button
              class="capture-btn"
              onclick={startCapture}
            >
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
                <rect x="2" y="3" width="20" height="14" rx="2" stroke="currentColor" stroke-width="2"/>
                <path d="M8 21h8M12 17v4" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
              </svg>
              Start Capture
            </button>
          {:else if connectionMode === "server"}
            <button
              class="connect-btn"
              onclick={startServer}
              disabled={connectionStatus === "connecting"}
            >
              {#if connectionStatus === "connecting"}
                <span class="spinner"></span>
                Starting...
              {:else}
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
                  <path d="M5 12h14" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
                </svg>
                Start Server
              {/if}
            </button>
          {:else}
            <button
              class="connect-btn"
              onclick={connect}
              disabled={connectionStatus === "connecting"}
            >
              {#if connectionStatus === "connecting"}
                <span class="spinner"></span>
                Connecting...
              {:else}
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
                  <path d="M5 12h14M12 5l7 7-7 7" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                Connect
              {/if}
            </button>
          {/if}
        </div>

        {#if availableDisplays.length > 1}
          <div class="display-selector">
            <label for="display-select">Display:</label>
            <select 
              id="display-select" 
              bind:value={selectedDisplay}
            >
              {#each availableDisplays as [name, width, height], i}
                <option value={i}>{name} ({width}x{height})</option>
              {/each}
            </select>
          </div>
        {/if}

        {#if errorMsg}
          <div class="error-message">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
              <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2"/>
              <path d="M12 8v4M12 16h.01" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
            </svg>
            {errorMsg}
          </div>
        {/if}

        <div class="status-bar">
          <span class="status-dot" class:connected={connectionStatus === "connected"}></span>
          {statusMessage}
        </div>
      </div>

      <div class="features">
        <div class="feature">
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
            <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" stroke="#3b82f6" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
          <h3>QUIC Powered</h3>
          <p>Ultra-low latency streaming</p>
        </div>
        <div class="feature">
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
            <rect x="2" y="3" width="20" height="14" rx="2" stroke="#10b981" stroke-width="2"/>
            <path d="M8 21h8M12 17v4" stroke="#10b981" stroke-width="2" stroke-linecap="round"/>
          </svg>
          <h3>ScreenCaptureKit</h3>
          <p>Native macOS capture API</p>
        </div>
        <div class="feature">
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" stroke="#8b5cf6" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
          <h3>Full Input</h3>
          <p>Mouse & keyboard support</p>
        </div>
      </div>
    </div>
  {/if}

  <!-- Remote Session View -->
  {#if isCapturing}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="session" role="toolbar"
         onmouseenter={() => resetToolbarTimeout()}
         onmousemove={() => resetToolbarTimeout()}>
      <!-- Floating Toolbar (Pill) -->
      <div class="toolbar" class:visible={showToolbar}>
        <button class="toolbar-btn" onclick={stopCapture} title="Disconnect">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none">
            <path d="M18 6L6 18M6 6l12 12" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
          </svg>
        </button>
        <div class="toolbar-divider"></div>
        <!-- SECURITY: View-only toggle -->
        <button
          class="toolbar-btn"
          class:active={!viewOnlyMode && inputConsentGiven}
          onclick={async () => {
            if (viewOnlyMode && inputConsentGiven) {
              viewOnlyMode = false;
              statusMessage = "Remote control enabled";
              // SEC-3: Enforce at backend too
              try { await invoke("grant_input_consent"); } catch(e) {}
            } else {
              viewOnlyMode = true;
              statusMessage = "View-only mode";
              // SEC-3: Revoke at backend too
              try { await invoke("revoke_input_consent"); } catch(e) {}
            }
          }}
          title={viewOnlyMode ? "Enable remote control" : "Disable remote control"}
        >
          {#if viewOnlyMode || !inputConsentGiven}
            🔒
          {:else}
            🔓
          {/if}
        </button>
        <div class="toolbar-divider"></div>
        <div class="vital-stats">
          <span class="stat">{fps} FPS</span>
          <span class="stat-divider">•</span>
          <span class="stat">{latency}ms</span>
        </div>
      </div>

      <!-- Remote Screen -->
      <div class="canvas-container">
        <canvas
          bind:this={canvasRef}
          onmousemove={handleMouseMove}
          onmousedown={handleMouseDown}
          onmouseup={handleMouseUp}
          onwheel={handleWheel}
          oncontextmenu={preventContextMenu}
        ></canvas>
        {#if fps === 0}
          <div class="waiting-message">
            <span class="spinner-large"></span>
            <p>Waiting for screen capture...</p>
            <p class="hint">Make sure to grant screen recording permissions</p>
          </div>
        {/if}
        
        <!-- SECURITY: View-only overlay -->
        {#if viewOnlyMode || !inputConsentGiven}
          <div class="security-overlay">
            <div class="security-notice">
              <svg width="32" height="32" viewBox="0 0 24 24" fill="none">
                <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" stroke="currentColor" stroke-width="2"/>
              </svg>
              <h3>View-Only Mode</h3>
              {#if !inputConsentGiven}
                <p>Remote control requires your explicit consent</p>
                <button class="consent-btn" onclick={() => showInputConsent = true}>
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
                    <path d="M9 12l2 2 4-4" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
                  </svg>
                  Enable Remote Control
                </button>
              {:else}
                <p>Remote control enabled. Toggle in toolbar above.</p>
              {/if}
            </div>
          </div>
        {/if}
      </div>
      
      <!-- Network Vitality Sparkline -->
      <div class="sparkline">
        <svg width="60" height="30" viewBox="0 0 60 30">
          <polyline
            points="0,15 10,10 20,20 30,8 40,12 50,5 60,15"
            fill="none"
            stroke="#3b82f6"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        <span>{fps} FPS</span>
      </div>
    </div>
  {/if}

  <!-- Onboarding Wizard -->
  {#if !hasCompletedOnboarding && connectionStatus === "disconnected" && !isCapturing}
    <div class="modal-backdrop" transition:fade>
      <div class="modal onboarding-modal" transition:slide>
        <div class="modal-header">
          <h3>🔧 Setup VibeRemote</h3>
        </div>
        <div class="modal-body">
          {#if onboardingStep === 1}
            <p>Welcome to VibeRemote! Let's get you set up.</p>
            <div class="onboarding-step">
              <div class="step-icon">1</div>
              <p><strong>Screen Recording</strong></p>
              <p class="step-desc">Required to capture your screen for remote viewing.</p>
              <button class="btn-primary" onclick={async () => {
                try {
                  await invoke("start_capture");
                  onboardingStep = 2;
                } catch (err) {
                  const error = err as Error;
                  errorMsg = error.message;
                  // Open system settings if permission denied
                  if (error.message.includes("permission") || error.message.includes("Permission")) {
                    await invoke("init_vibe");
                  }
                }
              }}>
                Test Screen Capture
              </button>
            </div>
          {:else if onboardingStep === 2}
            <div class="onboarding-step">
              <div class="step-icon">2</div>
              <p><strong>Remote Control</strong></p>
              <p class="step-desc">Allows the remote peer to control your mouse and keyboard.</p>
              <button class="btn-secondary" onclick={() => {
                onboardingStep = 3;
              }}>
                Continue
              </button>
            </div>
          {:else if onboardingStep === 3}
            <div class="onboarding-step">
              <div class="step-icon">✓</div>
              <p><strong>You're All Set!</strong></p>
              <p class="step-desc">VibeRemote is ready to use.</p>
              <button class="btn-primary" onclick={() => {
                saveOnboardingState();
              }}>
                Get Started
              </button>
            </div>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <!-- CRIT-2: Input Consent Modal -->
  {#if showInputConsent}
    <div class="modal-backdrop" transition:fade>
      <div class="modal" transition:slide>
        <div class="modal-header">
          <h3>🔐 Enable Remote Control</h3>
          <button class="modal-close" onclick={() => showInputConsent = false}>×</button>
        </div>
        <div class="modal-body">
          <p class="consent-warning">
            <strong>Warning:</strong> Enabling remote control allows the connected peer to:
          </p>
          <ul class="consent-list">
            <li>Move your mouse cursor anywhere on screen</li>
            <li>Click buttons, links, and interact with applications</li>
            <li>Type text using your keyboard</li>
            <li>Use keyboard shortcuts (including system commands)</li>
          </ul>
          <p class="consent-advisory">
            Only enable this if you trust the remote peer and will be monitoring the session.
          </p>
        </div>
        <div class="modal-footer">
          <button class="btn-secondary" onclick={() => showInputConsent = false}>
            Cancel
          </button>
          <button class="btn-primary" onclick={grantInputConsent}>
            I Understand - Enable Remote Control
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- CRIT-3: Clipboard Consent Modal -->
  {#if showClipboardConsent}
    <div class="modal-backdrop" transition:fade>
      <div class="modal" transition:slide>
        <div class="modal-header">
          <h3>📋 Enable Clipboard Sync</h3>
          <button class="modal-close" onclick={() => showClipboardConsent = false}>×</button>
        </div>
        <div class="modal-body">
          <p class="consent-warning">
            <strong>Warning:</strong> Enabling clipboard sync allows:
          </p>
          <ul class="consent-list">
            <li>Remote peer can read text from your clipboard</li>
            <li>Remote peer can modify your clipboard content</li>
            <li>This may include passwords, API keys, or sensitive data</li>
          </ul>
          <p class="consent-advisory">
            Only enable if necessary. You can disable this at any time.
          </p>
        </div>
        <div class="modal-footer">
          <button class="btn-secondary" onclick={() => showClipboardConsent = false}>
            Cancel
          </button>
          <button class="btn-primary" onclick={grantClipboardConsent}>
            I Understand - Enable Clipboard
          </button>
        </div>
      </div>
    </div>
  {/if}
</main>

<style>
  :global(:root) {
    --slate-50: #f8fafc;
    --slate-100: #f1f5f9;
    --slate-200: #e2e8f0;
    --slate-300: #cbd5e1;
    --slate-400: #94a3b8;
    --slate-500: #64748b;
    --slate-600: #475569;
    --slate-700: #334155;
    --slate-800: #1e293b;
    --slate-900: #0f172a;
    --slate-950: #020617;
    
    --blue-500: #3b82f6;
    --blue-600: #2563eb;
    --green-500: #10b981;
    --purple-500: #8b5cf6;
    --red-500: #ef4444;
    
    font-family: 'Inter', system-ui, -apple-system, sans-serif;
    margin: 0;
    padding: 0;
  }

  :global(.deep-slate) {
    min-height: 100vh;
    background: linear-gradient(135deg, var(--slate-950) 0%, var(--slate-900) 50%, var(--slate-800) 100%);
    color: var(--slate-100);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
  }

  /* Dashboard Styles */
  :global(.dashboard) {
    max-width: 480px;
    width: 100%;
    text-align: center;
  }

  :global(.logo-section) {
    margin-bottom: 2.5rem;
  }

  :global(.logo-icon) {
    margin-bottom: 1rem;
  }

  h1 {
    font-size: 2.5rem;
    font-weight: 700;
    margin: 0.5rem 0 0.25rem;
    background: linear-gradient(135deg, var(--slate-100) 0%, var(--blue-500) 100%);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  :global(.subtitle) {
    color: var(--slate-400);
    font-size: 1rem;
    font-weight: 400;
    margin: 0;
  }

  :global(.connection-card) {
    background: rgba(30, 41, 59, 0.6);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(100, 116, 139, 0.2);
    border-radius: 1.5rem;
    padding: 2rem;
    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.25);
  }

  .mode-selector {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1.5rem;
    background: rgba(15, 23, 42, 0.4);
    padding: 0.5rem;
    border-radius: 1rem;
  }

  .mode-selector button {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 0.75rem;
    font-size: 0.875rem;
    font-weight: 600;
    font-family: inherit;
    background: transparent;
    border: none;
    border-radius: 0.75rem;
    color: var(--slate-400);
    cursor: pointer;
    transition: all 0.2s;
  }

  .mode-selector button.active {
    background: var(--blue-500);
    color: white;
  }

  .mode-selector button:hover:not(.active) {
    background: rgba(100, 116, 139, 0.2);
    color: var(--slate-200);
  }

  .input-group {
    text-align: left;
    margin-bottom: 1.5rem;
  }

  label {
    display: block;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--slate-300);
    margin-bottom: 0.5rem;
  }

  input {
    width: 100%;
    padding: 0.875rem 1rem;
    font-size: 1rem;
    font-family: inherit;
    background: rgba(15, 23, 42, 0.6);
    border: 1px solid var(--slate-600);
    border-radius: 0.75rem;
    color: var(--slate-100);
    transition: all 0.2s;
    box-sizing: border-box;
  }

  input:focus {
    outline: none;
    border-color: var(--blue-500);
    box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.1);
  }

  input::placeholder {
    color: var(--slate-500);
  }

  .button-row {
    display: flex;
    gap: 0.75rem;
  }

  .connect-btn, .capture-btn {
    flex: 1;
    padding: 1rem;
    font-size: 1rem;
    font-weight: 600;
    font-family: inherit;
    border: none;
    border-radius: 0.75rem;
    cursor: pointer;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
  }

  .connect-btn {
    background: linear-gradient(135deg, var(--blue-500) 0%, var(--blue-600) 100%);
    color: white;
  }

  .capture-btn {
    background: linear-gradient(135deg, var(--green-500) 0%, #059669 100%);
    color: white;
  }

  .connect-btn:hover:not(:disabled), .capture-btn:hover {
    transform: translateY(-2px);
    box-shadow: 0 10px 25px -5px rgba(59, 130, 246, 0.4);
  }

  .capture-btn:hover {
    box-shadow: 0 10px 25px -5px rgba(16, 185, 129, 0.4);
  }

  .connect-btn:active:not(:disabled), .capture-btn:active {
    transform: translateY(0);
  }

  .connect-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  .spinner-large {
    width: 48px;
    height: 48px;
    border: 4px solid rgba(59, 130, 246, 0.2);
    border-top-color: var(--blue-500);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-message {
    margin-top: 1rem;
    padding: 0.75rem;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: 0.5rem;
    color: var(--red-500);
    font-size: 0.875rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .status-bar {
    margin-top: 1rem;
    padding: 0.5rem;
    font-size: 0.75rem;
    color: var(--slate-400);
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--slate-500);
  }

  .status-dot.connected {
    background: var(--green-500);
    box-shadow: 0 0 8px var(--green-500);
  }

  .display-selector {
    margin-top: 1rem;
    text-align: left;
  }

  .display-selector label {
    display: block;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--slate-300);
    margin-bottom: 0.5rem;
  }

  .display-selector select {
    width: 100%;
    padding: 0.75rem;
    font-size: 0.875rem;
    font-family: inherit;
    background: rgba(15, 23, 42, 0.6);
    border: 1px solid var(--slate-600);
    border-radius: 0.5rem;
    color: var(--slate-100);
    cursor: pointer;
  }

  .display-selector select:focus {
    outline: none;
    border-color: var(--blue-500);
  }

  .features {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1.5rem;
    margin-top: 3rem;
  }

  .feature {
    text-align: center;
  }

  .feature svg {
    margin-bottom: 0.75rem;
  }

  .feature h3 {
    font-size: 0.875rem;
    font-weight: 600;
    color: var(--slate-200);
    margin: 0 0 0.25rem;
  }

  .feature p {
    font-size: 0.75rem;
    color: var(--slate-400);
    margin: 0;
  }

  /* Session View */
  .session {
    position: relative;
    width: 100%;
    max-width: 1400px;
  }

  .toolbar {
    position: fixed;
    top: 1.5rem;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(30, 41, 59, 0.8);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(100, 116, 139, 0.2);
    border-radius: 9999px;
    padding: 0.75rem 1.5rem;
    display: flex;
    align-items: center;
    gap: 1rem;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.3);
    transition: all 0.3s ease;
    z-index: 100;
  }

  .toolbar.visible {
    opacity: 1;
  }

  .toolbar:not(.visible) {
    opacity: 0;
    pointer-events: none;
  }

  .toolbar-btn {
    background: transparent;
    border: none;
    color: var(--slate-300);
    cursor: pointer;
    padding: 0.5rem;
    border-radius: 0.5rem;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .toolbar-btn:hover {
    background: rgba(100, 116, 139, 0.2);
    color: var(--slate-100);
  }

  .toolbar-divider {
    width: 1px;
    height: 20px;
    background: var(--slate-600);
  }

  .vital-stats {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--slate-300);
  }

  .stat {
    font-variant-numeric: tabular-nums;
  }

  .stat-divider {
    color: var(--slate-500);
  }

  .canvas-container {
    position: relative;
    border-radius: 1rem;
    overflow: hidden;
    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
    background: var(--slate-950);
  }

  canvas {
    width: 100%;
    height: auto;
    display: block;
    cursor: crosshair;
  }

  .waiting-message {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    text-align: center;
    color: var(--slate-400);
  }

  .waiting-message p {
    margin-top: 1rem;
  }

  .hint {
    font-size: 0.875rem;
    color: var(--slate-500);
  }

  /* SECURITY: View-only overlay styles */
  .security-overlay {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(2, 6, 23, 0.7);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10;
  }

  .security-notice {
    background: rgba(30, 41, 59, 0.95);
    border: 1px solid var(--slate-600);
    border-radius: 1.5rem;
    padding: 2.5rem;
    text-align: center;
    max-width: 400px;
    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
  }

  .security-notice h3 {
    font-size: 1.25rem;
    font-weight: 600;
    margin: 1rem 0 0.5rem;
    color: var(--slate-100);
  }

  .security-notice p {
    color: var(--slate-300);
    margin-bottom: 1.5rem;
  }

  .consent-btn {
    background: linear-gradient(135deg, var(--blue-500), var(--blue-600));
    color: white;
    border: none;
    padding: 0.875rem 1.5rem;
    border-radius: 0.75rem;
    font-weight: 600;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    transition: all 0.2s;
  }

  .consent-btn:hover {
    transform: translateY(-2px);
    box-shadow: 0 10px 25px -5px rgba(59, 130, 246, 0.4);
  }

  /* SECURITY: Modal styles for consent dialogs */
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(2, 6, 23, 0.8);
    backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: var(--slate-800);
    border: 1px solid var(--slate-600);
    border-radius: 1.5rem;
    max-width: 500px;
    width: 90%;
    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1.5rem 2rem;
    border-bottom: 1px solid var(--slate-700);
  }

  .modal-header h3 {
    font-size: 1.25rem;
    font-weight: 600;
    margin: 0;
    color: var(--slate-100);
  }

  .modal-close {
    background: transparent;
    border: none;
    color: var(--slate-400);
    font-size: 1.5rem;
    cursor: pointer;
    padding: 0.25rem;
    line-height: 1;
  }

  .modal-close:hover {
    color: var(--slate-100);
  }

  .modal-body {
    padding: 2rem;
  }

  .consent-warning {
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: 0.75rem;
    padding: 1rem;
    color: var(--red-500);
    margin-top: 0;
  }

  .consent-list {
    margin: 1.5rem 0;
    padding-left: 1.5rem;
    color: var(--slate-300);
  }

  .consent-list li {
    margin-bottom: 0.75rem;
  }

  .consent-advisory {
    color: var(--slate-400);
    font-size: 0.875rem;
    font-style: italic;
  }

  .modal-footer {
    display: flex;
    gap: 1rem;
    padding: 1.5rem 2rem;
    border-top: 1px solid var(--slate-700);
    justify-content: flex-end;
  }

  .btn-secondary {
    background: var(--slate-700);
    color: var(--slate-200);
    border: none;
    padding: 0.75rem 1.5rem;
    border-radius: 0.5rem;
    cursor: pointer;
    font-weight: 500;
  }

  .btn-secondary:hover {
    background: var(--slate-600);
  }

  .btn-primary {
    background: linear-gradient(135deg, var(--blue-500), var(--blue-600));
    color: white;
    border: none;
    padding: 0.75rem 1.5rem;
    border-radius: 0.5rem;
    cursor: pointer;
    font-weight: 600;
  }

  .btn-primary:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px -2px rgba(59, 130, 246, 0.4);
  }

  .sparkline {
    position: fixed;
    bottom: 1.5rem;
    right: 1.5rem;
    background: rgba(30, 41, 59, 0.8);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    border: 1px solid rgba(100, 116, 139, 0.2);
    border-radius: 0.75rem;
    padding: 0.75rem 1rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.75rem;
    font-weight: 500;
    color: var(--slate-300);
    box-shadow: 0 10px 15px -3px rgba(0, 0, 0, 0.3);
  }

  @media (max-width: 640px) {
    .features {
      grid-template-columns: 1fr;
    }
    
    :global(.deep-slate) {
      padding: 1rem;
    }
    
    h1 {
      font-size: 2rem;
    }

    .button-row {
      flex-direction: column;
    }
  }

  .onboarding-step {
    text-align: center;
    padding: 1.5rem;
  }

  .step-icon {
    width: 48px;
    height: 48px;
    background: var(--blue-500);
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 1.5rem;
    font-weight: bold;
    margin: 0 auto 1rem;
  }

  .step-desc {
    color: var(--slate-400);
    font-size: 0.875rem;
    margin-bottom: 1rem;
  }
</style>
