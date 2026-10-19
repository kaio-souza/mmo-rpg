//! `replay`: runs a zone from its replay log, without network or database,
//! tick by tick, and compares the state hash with the recorded one.
//!
//! ```text
//! replay zone-load_test-1763042400.replay --content content/ [--until-tick N] [--realtime]
//! ```
//!
//! `--realtime` paces ticks at 20 Hz, like the server (timings then include
//! the cost of waking idle threads and cores).
//!
//!

use replay::Replay;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "usage: replay <file.replay> --content <folder> [--until-tick N] [--realtime]";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut file = None;
    let mut content = None;
    let mut until = u64::MAX;
    let mut realtime = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--content" => content = it.next().map(PathBuf::from),
            "--realtime" => realtime = true,
            "--until-tick" => {
                until = it.next().and_then(|v| v.parse().ok()).ok_or(USAGE)?;
            }
            f if !f.starts_with('-') && file.is_none() => file = Some(PathBuf::from(f)),
            _ => return Err(USAGE.into()),
        }
    }
    let (Some(file), Some(content)) = (file, content) else {
        return Err(USAGE.into());
    };

    let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut replay = Replay::open(&bytes, &content)?;
    println!(
        "zone {} seed {} from tick {} ({} records)",
        replay.info.zone, replay.info.seed, replay.info.first_tick, replay.info.records
    );

    let (mut total, mut worst, mut ticks) = (Duration::ZERO, Duration::ZERO, 0u32);
    while replay.next_record().is_some_and(|r| r.tick.0 <= until) {
        let Some(step) = replay.step() else { break };
        if realtime {
            std::thread::sleep(Duration::from_millis(50).saturating_sub(step.took));
        }
        total += step.took;
        worst = worst.max(step.took);
        ticks += 1;
        if let Some((hash, recorded)) = step.diverged {
            return Err(format!(
                "tick {}: state hash {hash:016x}, recorded {recorded:016x}: the simulation diverged",
                step.tick
            ));
        }
    }
    println!(
        "ok: {ticks} ticks reproduced with identical state hashes; tick mean {} us, worst {} us; {} players at the end",
        (total / ticks.max(1)).as_micros(),
        worst.as_micros(),
        replay.zone().player_count()
    );
    Ok(())
}
