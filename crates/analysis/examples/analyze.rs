// SPDX-License-Identifier: AGPL-3.0-or-later

//! Analyzes files and prints the result, for tuning the analysis settings:
//!
//!     cargo run --release -p heron-analysis --example analyze -- FILE...
//!
//! Settings come from `HERON_ANALYSIS_SETTINGS` (JSON, same format as the
//! `analysis` row of the `settings` table), defaults otherwise.

use std::path::Path;
use std::time::Instant;

use heron_core::AnalysisSettings;

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let settings: AnalysisSettings = match std::env::var("HERON_ANALYSIS_SETTINGS") {
        Ok(json) => match serde_json::from_str(&json) {
            Ok(settings) => settings,
            Err(error) => {
                eprintln!("invalid HERON_ANALYSIS_SETTINGS: {error}");
                std::process::exit(2);
            }
        },
        Err(_) => AnalysisSettings::default(),
    };
    for file in std::env::args().skip(1) {
        let started = Instant::now();
        match heron_analysis::analyze_file(Path::new(&file), &settings) {
            Ok(a) => println!(
                "{file}\n  duration {:.1} s, {:.1} LUFS, peak {:.1} dBTP, cue-in {:.2} s, \
                 cue-out {:.2} s (-{:.2} s), fade at -{:.2} s{} [{} ms]",
                f64::from(a.duration_ms) / 1000.0,
                a.integrated_lufs.unwrap_or(f64::NAN),
                a.true_peak_db.unwrap_or(f64::NAN),
                f64::from(a.cue_in_ms) / 1000.0,
                f64::from(a.cue_out_ms) / 1000.0,
                f64::from(a.duration_ms - a.cue_out_ms) / 1000.0,
                f64::from(a.cue_out_ms - a.fade_out_start_ms.unwrap_or(a.cue_out_ms)) / 1000.0,
                if a.has_internal_silence {
                    ", internal silence"
                } else {
                    ""
                },
                started.elapsed().as_millis(),
            ),
            Err(error) => println!("{file}\n  error: {error}"),
        }
    }
}
