//! Windows backend: WASAPI in shared, event-driven mode, through
//! the `wasapi` crate (no system library at build time). The mixer runs on
//! a thread of its own; the device converts our 44 100 Hz float stereo to
//! its own format (`autoconvert`).

use crate::{Mixer, Order, RATE, feed};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, sync_channel};
use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

/// Stops the audio thread when the client drops [`crate::Audio`].
struct Stop(Arc<AtomicBool>);

impl Drop for Stop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Opens the default output device; the thread answers whether it could.
pub(crate) fn open(orders: Receiver<Order>) -> Result<Box<dyn std::any::Any>, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = sync_channel(1);
    let flag = stop.clone();
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            let mut out = Vec::new();
            match start() {
                Ok((client, render, event, align)) => {
                    let _ = ready_tx.send(Ok(()));
                    let mut mixer = Mixer::new();
                    while !flag.load(Ordering::Relaxed) {
                        if event.wait_for_event(1000).is_err() {
                            continue;
                        }
                        let Ok(frames) = client.get_available_space_in_frames() else {
                            break;
                        };
                        out.resize(frames as usize * 2, 0.0);
                        feed(&mut mixer, &orders, &mut out);
                        let bytes: Vec<u8> = out.iter().flat_map(|s| s.to_le_bytes()).collect();
                        debug_assert_eq!(bytes.len(), frames as usize * align);
                        if render.write_to_device(&bytes, None).is_err() {
                            break;
                        }
                    }
                    let _ = client.stop_stream();
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    ready_rx
        .recv()
        .map_err(|_| "the audio thread ended before opening".to_string())??;
    Ok(Box::new(Stop(stop)))
}

type Opened = (
    wasapi::AudioClient,
    wasapi::AudioRenderClient,
    wasapi::Handle,
    usize,
);

/// Turns a WASAPI error into a line for the log.
fn e(what: &'static str) -> impl Fn(wasapi::WasapiError) -> String {
    move |err| format!("{what}: {err}")
}

fn start() -> Result<Opened, String> {
    let _ = wasapi::initialize_mta();
    let device = DeviceEnumerator::new()
        .map_err(e("no device list"))?
        .get_default_device(Direction::Render)
        .map_err(e("no output device"))?;
    let mut client = device.get_iaudioclient().map_err(e("no audio client"))?;
    let format = WaveFormat::new(32, 32, SampleType::Float, RATE as usize, 2, None);
    let align = format.get_blockalign() as usize;
    let (default_period, _) = client.get_device_period().map_err(e("no device period"))?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: default_period,
    };
    client
        .initialize_client(&format, Direction::Render, mode, None)
        .map_err(e("cannot open the stream"))?;
    let event = client.set_get_eventhandle().map_err(e("no event"))?;
    let render = client.get_audiorenderclient().map_err(e("no render client"))?;
    // Silence first, so the stream starts with a full buffer.
    let frames = client.get_available_space_in_frames().map_err(e("no buffer"))?;
    render
        .write_to_device(&vec![0u8; frames as usize * align], None)
        .map_err(e("cannot write"))?;
    client.start_stream().map_err(e("cannot start"))?;
    Ok((client, render, event, align))
}
