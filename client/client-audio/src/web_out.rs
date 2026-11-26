//! Browser backend: WebAudio. A script node asks for frames on the page's
//! thread and the mixer fills them. Browsers keep sound off until the
//! player clicks or types, so the first such event starts it.

use crate::{Mixer, Order, RATE, feed};
use std::sync::mpsc::Receiver;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{AudioContext, AudioContextOptions, AudioProcessingEvent, Event};

/// Frames per request: about 46 ms at 44 100 Hz.
const BLOCK: u32 = 2048;

struct Backend {
    ctx: AudioContext,
    _process: Closure<dyn FnMut(AudioProcessingEvent)>,
    _wake: Closure<dyn FnMut(Event)>,
}

impl Drop for Backend {
    fn drop(&mut self) {
        let _ = self.ctx.close();
    }
}

pub(crate) fn open(orders: Receiver<Order>) -> Result<Box<dyn std::any::Any>, String> {
    let opts = AudioContextOptions::new();
    opts.set_sample_rate(RATE as f32);
    let ctx = AudioContext::new_with_context_options(&opts).map_err(|e| format!("no WebAudio: {e:?}"))?;
    let node = ctx
        .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
            BLOCK, 0, 2,
        )
        .map_err(|e| format!("sound stream refused: {e:?}"))?;
    let mut mixer = Mixer::new();
    let mut frames: Vec<f32> = Vec::new();
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let process: Closure<dyn FnMut(AudioProcessingEvent)> = Closure::new(move |e: AudioProcessingEvent| {
        let Ok(out) = e.output_buffer() else { return };
        let n = out.length() as usize;
        frames.resize(2 * n, 0.0);
        feed(&mut mixer, &orders, &mut frames);
        left.clear();
        right.clear();
        for f in frames.chunks_exact(2) {
            left.push(f.first().copied().unwrap_or(0.0));
            right.push(f.last().copied().unwrap_or(0.0));
        }
        let _ = out.copy_to_channel(&left, 0);
        let _ = out.copy_to_channel(&right, 1);
    });
    node.set_onaudioprocess(Some(process.as_ref().unchecked_ref()));
    node.connect_with_audio_node(&ctx.destination())
        .map_err(|e| format!("sound stream refused: {e:?}"))?;
    let c = ctx.clone();
    let wake: Closure<dyn FnMut(Event)> = Closure::new(move |_: Event| {
        let _ = c.resume();
    });
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        for kind in ["pointerdown", "keydown"] {
            let _ = doc.add_event_listener_with_callback(kind, wake.as_ref().unchecked_ref());
        }
    }
    Ok(Box::new(Backend {
        ctx,
        _process: process,
        _wake: wake,
    }))
}
