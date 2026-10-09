//! Linux backend: the PulseAudio protocol in pure Rust (`pulseaudio`
//! crate), which PipeWire also speaks. The crate runs its own
//! thread and calls us back whenever the server wants frames.

use crate::{Mixer, Order, RATE, feed};
use pulseaudio::{AsPlaybackSource, Client, protocol};
use std::sync::mpsc::Receiver;

/// What the server keeps queued: about 60 ms, so a blow is heard with the
/// hit and not after it.
const TARGET_MS: u32 = 60;
/// Bytes per frame: two channels of 16 bits.
const FRAME_BYTES: u32 = 4;

pub(crate) fn open(orders: Receiver<Order>, app: &str) -> Result<Box<dyn std::any::Any>, String> {
    let name = std::ffi::CString::new(app.replace('\0', "")).map_err(|e| format!("sound: {e}"))?;
    let client = Client::from_env(&name).map_err(|e| format!("no sound server: {e}"))?;
    let bytes_per_ms = RATE / 1000 * FRAME_BYTES;
    let params = protocol::PlaybackStreamParams {
        sample_spec: protocol::SampleSpec {
            format: protocol::SampleFormat::S16Le,
            channels: 2,
            sample_rate: RATE,
        },
        channel_map: protocol::ChannelMap::stereo(),
        cvolume: Some(protocol::ChannelVolume::norm(2)),
        sink_name: Some(protocol::DEFAULT_SINK.to_owned()),
        buffer_attr: protocol::stream::BufferAttr {
            target_length: TARGET_MS * bytes_per_ms,
            minimum_request_length: 10 * bytes_per_ms,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut mixer = Mixer::new();
    let mut frames: Vec<f32> = Vec::new();
    // Called on the crate's thread; never returns 0, which would end the stream.
    let source = move |buf: &mut [u8]| -> usize {
        let n = buf.len() / FRAME_BYTES as usize;
        frames.resize(2 * n, 0.0);
        feed(&mut mixer, &orders, &mut frames);
        for (out, s) in buf.chunks_exact_mut(2).zip(&frames) {
            let v = (s * 32767.0) as i16;
            out.copy_from_slice(&v.to_le_bytes());
        }
        n * FRAME_BYTES as usize
    };
    let stream =
        futures::executor::block_on(client.create_playback_stream(params, source.as_playback_source()))
            .map_err(|e| format!("sound stream refused: {e}"))?;
    Ok(Box::new((client, stream)))
}
