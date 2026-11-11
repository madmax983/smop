//! Example: Process files in parallel with a progress bar.
//!
//! Run with: `cargo run --example file_processor`

use smop::prelude::*;
use std::thread;
use std::time::Duration;

fn main() {
    // Simulate finding files to process
    let files = vec![
        "config.json",
        "data.csv",
        "report.txt",
        "backup.sql",
        "logs.txt",
    ];

    println!("Processing {} files...\n", files.len());

    // Threads share one bar: `print::Progress` is `Sync` and `Clone`.
    let progress: print::Progress = print::progress(files.len() as u64);
    thread::scope(|scope| {
        for file in &files {
            let progress = &progress;
            scope.spawn(move || {
                progress.set_message(format!("Processing {file}"));
                // Simulate work
                thread::sleep(Duration::from_millis(500));
                progress.inc(1);
            });
        }
    });

    progress.finish_with_message("Complete!");

    success!("Processed {} files successfully", files.len());
}
