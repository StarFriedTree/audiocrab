use std::process::Command;

use url::Url;

pub fn download_from_link(link: &str) {
    eprintln!("in download route");

    let link = Url::parse(link).expect("Invalid URL").to_string();
    let output = Command::new("yt-dlp")
        .args([
            "--extract-audio",
            "--audio-format",
            "vorbis",
            "-P",
            "downloads/temp/",
            "--retries",
            "10",
            "--continue",
            "--ignore-errors",
            "-k",
            &link,
        ])
        .output()
        .expect("failed to download from link");

    let _stdout = output.stdout;
}