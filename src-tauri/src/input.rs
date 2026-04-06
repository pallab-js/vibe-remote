//! Input injection module using enigo
//!
//! Handles remote mouse and keyboard input simulation
//! with cross-platform support.

use enigo::{Enigo, Key, Keyboard, Mouse, Button, Coordinate, Direction, Axis};
use tracing::{info, debug};
use std::sync::{Arc, Mutex};

use crate::error::{VibeResult, VibeError};

/// Mouse event types
#[derive(Debug, Clone)]
pub enum MouseEvent {
    Move { x: i32, y: i32 },
    Down { button: MouseButton },
    Up { button: MouseButton },
    Wheel { delta: i32 },
}

/// Mouse button types
#[derive(Debug, Clone)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Keyboard event types
#[derive(Debug, Clone)]
pub enum KeyboardEvent {
    KeyDown { key: VirtualKey },
    KeyUp { key: VirtualKey },
    Text { text: String },
}

/// Virtual key codes - comprehensive mapping
#[derive(Debug, Clone)]
pub enum VirtualKey {
    // Special keys
    Return,
    Tab,
    Backspace,
    Escape,
    Space,
    
    // Modifier keys
    Shift,
    Control,
    Alt,
    Meta, // Command on Mac, Windows on Win
    
    // Arrow keys
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    
    // Navigation
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    
    // Function keys
    F1, F2, F3, F4, F5, F6,
    F7, F8, F9, F10, F11, F12,
    
    // Regular characters
    Character(char),
}

impl VirtualKey {
    /// Parse a key string to VirtualKey
    pub fn from_str(key: &str) -> Option<Self> {
        match key.to_lowercase().as_str() {
            "return" | "enter" => Some(VirtualKey::Return),
            "tab" => Some(VirtualKey::Tab),
            "backspace" => Some(VirtualKey::Backspace),
            "escape" | "esc" => Some(VirtualKey::Escape),
            "space" => Some(VirtualKey::Space),
            "shift" => Some(VirtualKey::Shift),
            "control" | "ctrl" => Some(VirtualKey::Control),
            "alt" | "option" => Some(VirtualKey::Alt),
            "meta" | "cmd" | "command" | "super" | "windows" => Some(VirtualKey::Meta),
            "arrowup" | "up" => Some(VirtualKey::ArrowUp),
            "arrowdown" | "down" => Some(VirtualKey::ArrowDown),
            "arrowleft" | "left" => Some(VirtualKey::ArrowLeft),
            "arrowright" | "right" => Some(VirtualKey::ArrowRight),
            "home" => Some(VirtualKey::Home),
            "end" => Some(VirtualKey::End),
            "pageup" | "pgup" => Some(VirtualKey::PageUp),
            "pagedown" | "pgdn" => Some(VirtualKey::PageDown),
            "delete" | "del" => Some(VirtualKey::Delete),
            "insert" | "ins" => Some(VirtualKey::Insert),
            "f1" => Some(VirtualKey::F1),
            "f2" => Some(VirtualKey::F2),
            "f3" => Some(VirtualKey::F3),
            "f4" => Some(VirtualKey::F4),
            "f5" => Some(VirtualKey::F5),
            "f6" => Some(VirtualKey::F6),
            "f7" => Some(VirtualKey::F7),
            "f8" => Some(VirtualKey::F8),
            "f9" => Some(VirtualKey::F9),
            "f10" => Some(VirtualKey::F10),
            "f11" => Some(VirtualKey::F11),
            "f12" => Some(VirtualKey::F12),
            _ if key.len() == 1 => Some(VirtualKey::Character(key.chars().next().unwrap())),
            _ => None,
        }
    }
}

/// Thread-safe input handler for remote control
#[derive(Clone)]
pub struct InputHandler {
    enigo: Arc<Mutex<Enigo>>,
}

impl InputHandler {
    /// Create a new input handler
    pub fn new() -> VibeResult<Self> {
        info!("Initializing input handler");
        
        let enigo = Enigo::new(&enigo::Settings::default())
            .map_err(|e| VibeError::Input(format!("Failed to initialize enigo: {}", e)))?;

        Ok(Self { 
            enigo: Arc::new(Mutex::new(enigo)),
        })
    }

    /// Handle a mouse event
    pub fn handle_mouse_event(&self, event: MouseEvent) -> VibeResult<()> {
        let mut enigo = self.enigo.lock()
            .map_err(|e| VibeError::Input(format!("Failed to lock enigo: {}", e)))?;
        
        match event {
            MouseEvent::Move { x, y } => {
                debug!("Mouse move to ({}, {})", x, y);
                enigo.move_mouse(x, y, Coordinate::Abs)
                    .map_err(|e| VibeError::Input(format!("Mouse move failed: {}", e)))?;
            }
            MouseEvent::Down { button } => {
                debug!("Mouse button down: {:?}", button);
                let btn = match button {
                    MouseButton::Left => Button::Left,
                    MouseButton::Right => Button::Right,
                    MouseButton::Middle => Button::Middle,
                };
                enigo.button(btn, Direction::Press)
                    .map_err(|e| VibeError::Input(format!("Mouse down failed: {}", e)))?;
            }
            MouseEvent::Up { button } => {
                debug!("Mouse button up: {:?}", button);
                let btn = match button {
                    MouseButton::Left => Button::Left,
                    MouseButton::Right => Button::Right,
                    MouseButton::Middle => Button::Middle,
                };
                enigo.button(btn, Direction::Release)
                    .map_err(|e| VibeError::Input(format!("Mouse up failed: {}", e)))?;
            }
            MouseEvent::Wheel { delta } => {
                debug!("Mouse wheel: {}", delta);
                enigo.scroll(delta, Axis::Vertical)
                    .map_err(|e| VibeError::Input(format!("Scroll failed: {}", e)))?;
            }
        }
        Ok(())
    }

    /// Handle a keyboard event
    pub fn handle_keyboard_event(&self, event: KeyboardEvent) -> VibeResult<()> {
        let mut enigo = self.enigo.lock()
            .map_err(|e| VibeError::Input(format!("Failed to lock enigo: {}", e)))?;
        
        match event {
            KeyboardEvent::KeyDown { key } => {
                debug!("Key down: {:?}", key);
                if let Some(k) = Self::virtual_key_to_enigo(&key) {
                    enigo.key(k, Direction::Press)
                        .map_err(|e| VibeError::Input(format!("Key down failed: {}", e)))?;
                }
            }
            KeyboardEvent::KeyUp { key } => {
                debug!("Key up: {:?}", key);
                if let Some(k) = Self::virtual_key_to_enigo(&key) {
                    enigo.key(k, Direction::Release)
                        .map_err(|e| VibeError::Input(format!("Key up failed: {}", e)))?;
                }
            }
            KeyboardEvent::Text { text } => {
                debug!("Text input: {}", text);
                enigo.text(&text)
                    .map_err(|e| VibeError::Input(format!("Text input failed: {}", e)))?;
            }
        }
        Ok(())
    }

    /// Convert VirtualKey to enigo::Key
    fn virtual_key_to_enigo(key: &VirtualKey) -> Option<Key> {
        match key {
            VirtualKey::Return => Some(Key::Return),
            VirtualKey::Tab => Some(Key::Tab),
            VirtualKey::Backspace => Some(Key::Backspace),
            VirtualKey::Escape => Some(Key::Escape),
            VirtualKey::Shift => Some(Key::Shift),
            VirtualKey::Control => Some(Key::Control),
            VirtualKey::Alt => Some(Key::Alt),
            VirtualKey::Meta => Some(Key::Meta),
            VirtualKey::Space => Some(Key::Space),
            VirtualKey::ArrowUp => Some(Key::UpArrow),
            VirtualKey::ArrowDown => Some(Key::DownArrow),
            VirtualKey::ArrowLeft => Some(Key::LeftArrow),
            VirtualKey::ArrowRight => Some(Key::RightArrow),
            VirtualKey::Home => Some(Key::Home),
            VirtualKey::End => Some(Key::End),
            VirtualKey::PageUp => Some(Key::PageUp),
            VirtualKey::PageDown => Some(Key::PageDown),
            VirtualKey::Delete => Some(Key::Delete),
            VirtualKey::Insert => None, // Not supported in enigo 0.2
            VirtualKey::F1 => Some(Key::F1),
            VirtualKey::F2 => Some(Key::F2),
            VirtualKey::F3 => Some(Key::F3),
            VirtualKey::F4 => Some(Key::F4),
            VirtualKey::F5 => Some(Key::F5),
            VirtualKey::F6 => Some(Key::F6),
            VirtualKey::F7 => Some(Key::F7),
            VirtualKey::F8 => Some(Key::F8),
            VirtualKey::F9 => Some(Key::F9),
            VirtualKey::F10 => Some(Key::F10),
            VirtualKey::F11 => Some(Key::F11),
            VirtualKey::F12 => Some(Key::F12),
            VirtualKey::Character(_c) => {
                // For regular characters, we'll handle them via text() instead
                None
            }
        }
    }
}

// No Default implementation - must be explicitly created with error handling
// This prevents panics during app initialization
