/// Audio module - stub audio system (no actual playback)
/// This provides the interface for future audio integration with rodio or kira.

/// Sound events that can be triggered in the game
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundEvent {
    BlockBreak,
    BlockPlace,
    Footstep,
    Jump,
    Splash,
    Hit,
    Death,
    UIClick,
}

/// Stub audio engine - provides the interface without actual audio playback
pub struct AudioEngine {
    master_volume: f32,
    music_volume: f32,
    playing_music: bool,
}

impl AudioEngine {
    /// Create a new audio engine (no-op initialization)
    pub fn new() -> Self {
        Self {
            master_volume: 1.0,
            music_volume: 0.5,
            playing_music: false,
        }
    }

    /// Play a sound event (stub - just logs in debug builds)
    pub fn play_sound(&self, _sound: SoundEvent) {
        // No-op: would use rodio or kira to play the sound
        // In debug builds, we could log:
        // log::debug!("Playing sound: {:?}", sound);
    }

    /// Set the master volume (0.0 to 1.0)
    pub fn set_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
    }

    /// Get the current master volume
    pub fn volume(&self) -> f32 {
        self.master_volume
    }

    /// Set the music volume (0.0 to 1.0)
    pub fn set_music_volume(&mut self, volume: f32) {
        self.music_volume = volume.clamp(0.0, 1.0);
    }

    /// Get the current music volume
    pub fn music_volume(&self) -> f32 {
        self.music_volume
    }

    /// Check if music is currently playing
    pub fn is_playing_music(&self) -> bool {
        self.playing_music
    }
}
