//! Connection state machine

use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone, Debug, PartialEq)]
pub enum ConnectionPhase {
    Idle,
    Resolving,
    Connecting,
    Authenticating,
    AwaitingApproval,
    Connected,
    Reconnecting { attempt: u32, max: u32 },
    Disconnecting,
    Error { code: u32, message: String },
}

#[derive(Clone, Debug)]
pub struct ConnectionEvent {
    pub phase: ConnectionPhase,
    pub peer_id: Option<String>,
    pub server_fingerprint: Option<String>,
    pub latency_ms: Option<u32>,
}

impl Default for ConnectionEvent {
    fn default() -> Self {
        Self {
            phase: ConnectionPhase::Idle,
            peer_id: None,
            server_fingerprint: None,
            latency_ms: None,
        }
    }
}

use std::sync::atomic::{AtomicU32, Ordering};

pub struct ConnectionStateMachine {
    phase: Arc<Mutex<ConnectionPhase>>,
    attempt: AtomicU32,
    max_attempts: u32,
    peer_id: Arc<Mutex<Option<String>>>,
    fingerprint: Arc<Mutex<Option<String>>>,
    latency_ms: Arc<Mutex<Option<u32>>>,
}

impl ConnectionStateMachine {
    pub fn new(max_attempts: u32) -> Self {
        Self {
            phase: Arc::new(Mutex::new(ConnectionPhase::Idle)),
            attempt: AtomicU32::new(0),
            max_attempts,
            peer_id: Arc::new(Mutex::new(None)),
            fingerprint: Arc::new(Mutex::new(None)),
            latency_ms: Arc::new(Mutex::new(None)),
        }
    }

    pub fn get_phase(&self) -> ConnectionPhase {
        self.phase.lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| ConnectionPhase::Error {
                code: 1,
                message: "Failed to lock phase".to_string(),
            })
    }

    pub fn set_phase(&self, phase: ConnectionPhase) {
        if let Ok(mut guard) = self.phase.lock() {
            *guard = phase;
        }
    }

    pub fn get_event(&self) -> ConnectionEvent {
        ConnectionEvent {
            phase: self.get_phase(),
            peer_id: self.peer_id.lock()
                .map(|g| g.clone())
                .unwrap_or_else(|_| None),
            server_fingerprint: self.fingerprint.lock()
                .map(|g| g.clone())
                .unwrap_or_else(|_| None),
            latency_ms: self.latency_ms.lock()
                .map(|g| *g)
                .unwrap_or_else(|_| None),
        }
    }

    pub fn set_peer(&self, peer_id: String, fingerprint: Option<String>) {
        if let Ok(mut guard) = self.peer_id.lock() {
            *guard = Some(peer_id);
        }
        if let Ok(mut guard) = self.fingerprint.lock() {
            *guard = fingerprint;
        }
    }

    pub fn set_latency(&self, latency_ms: u32) {
        if let Ok(mut guard) = self.latency_ms.lock() {
            *guard = Some(latency_ms);
        }
    }

    pub fn next_attempt(&self) -> bool {
        let attempt = self.attempt.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt < self.max_attempts {
            self.set_phase(ConnectionPhase::Reconnecting {
                attempt,
                max: self.max_attempts,
            });
            true
        } else {
            self.set_phase(ConnectionPhase::Error {
                code: 1,
                message: "Max reconnection attempts reached".into(),
            });
            false
        }
    }

    pub fn reset(&self) {
        self.set_phase(ConnectionPhase::Idle);
        self.attempt.store(0, Ordering::SeqCst);
        *self.peer_id.lock().unwrap() = None;
        *self.fingerprint.lock().unwrap() = None;
        *self.latency_ms.lock().unwrap() = None;
    }
}

impl Default for ConnectionStateMachine {
    fn default() -> Self {
        Self::new(10)
    }
}
