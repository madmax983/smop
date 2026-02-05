//! Example: Process files with a progress bar.
//!
//! Run with: `cargo run --example file_processor`

#![allow(clippy::unnecessary_wraps)]

use smop::prelude::*;
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    // Simulate finding files to process
    let files = vec![
        "config.json",
        "data.csv",
        "report.txt",
        "backup.sql",
        "logs.txt",
    ];

    println!("Processing {} files...\n", files.len());

    let progress = print::progress(files.len() as u64);

    for file in &files {
        progress.set_message(format!("Processing {file}"));

        // Simulate work
        thread::sleep(Duration::from_millis(500));

        progress.inc(1);
    }

    progress.finish_with_message("Complete!");

    success!("Processed {} files successfully", files.len());

    Ok(())
}
