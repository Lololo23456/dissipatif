//! Plays the soundscape on the default output device, through cpal.
//!
//! The device asks for samples from its own thread, a little at a time (a few milliseconds per
//! buffer): the callback must answer at once, so it never waits on a lock nor allocates. The
//! game talks to it only through `sound::Shared` (atomics).

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::sound::{Shared, Soundscape};

pub struct Audio {
    /// Playing as long as it is kept.
    _stream: cpal::Stream,
    pub shared: Arc<Shared>,
}

impl Audio {
    /// Starts the sound, or explains why there is none (no device…): the game runs silent.
    pub fn start(seed: u32) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("pas de sortie audio")?;
        let supported = device
            .default_output_config()
            .map_err(|e| format!("configuration audio : {e}"))?;
        let config = supported.config();
        let channels = config.channels as usize;
        let shared = Arc::new(Shared::default());
        let mut soundscape = Soundscape::new(config.sample_rate as f32, shared.clone(), seed);
        let stream = device
            .build_output_stream::<f32, _, _>(
                config,
                move |out: &mut [f32], _| soundscape.fill(out, channels),
                |e| eprintln!("Erreur audio : {e}"),
                None,
            )
            .map_err(|e| format!("flux audio : {e}"))?;
        stream.play().map_err(|e| format!("lecture audio : {e}"))?;
        Ok(Self {
            _stream: stream,
            shared,
        })
    }
}
