use std::process::Command;

use url::Url;
use crate::cli::Cli;

pub fn download_from_link(link: &str, app_args: &Cli) {
    eprintln!("in metadata download route");

    let link = Url::parse(link).expect("Invalid URL").to_string();

    let mut arguments = vec! [
        "-f", "bestaudio/best",
            "--extract-audio",
            "--audio-format", "vorbis",
            "-P", "downloads/temp/",
            "--retries", "10",
            "--continue",
            "--ignore-errors",
            "--no-embed-thumbnail",
            "--embed-metadata",
            "--no-embed-chapters",
            "--write-thumbnail",
            "--convert-thumbnail", "jpg",
            "--write-info-json", 
    ];

    let browser_str;
    if let Some(browser) = &app_args.cookies_from_browser {
        browser_str = format!("{:?}", browser).to_lowercase();
        eprintln!("got browser: {}", browser_str);
        arguments.push("--cookies-from-browser");
        arguments.push(&browser_str);
    }
    arguments.push(&link);

    let output = Command::new("yt-dlp")
        .args(&arguments)
        .output()
        .expect("failed to download from link");

    println! ("stdout: {}", String::from_utf8_lossy(&output.stdout));
    eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
}

/// TODO: incomplete
pub fn download_without_metadata(link: &str) {
    eprintln!("in no metadata download route");
    let link = Url::parse(link).expect("Invalid URL").to_string();
    let output = Command::new("yt-dlp")
        .args([
            "-f", "bestaudio/best",
            "--extract-audio",
            "--audio-format", "vorbis",
            "-P", "downloads/temp/",
            "--retries", "10",
            "--continue",
            "--ignore-errors",
            "--no-embed-metadata",
            "--no-embed-thumbnail",
            "--no-embed-chapters",
            "--postprocessor-args", "ExtractAudio:-map_metadata -1 -fflags +bitexact",
            &link,
        ])
        .output()
        .expect("Failed to execute yt-dlp");

    println! ("stdout: {}", String::from_utf8_lossy(&output.stdout));
    eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
}
