use std::path::Path;
use std::process::Command;

use crate::Error;

/// MediaMTX HLS/WebRTC page for a typical local ingest URL.
#[must_use]
pub fn view_url(ingest: &str) -> String {
    let url = ingest.trim();
    if let Some(rest) = url.strip_prefix("rtsp://") {
        if let Some((host, path)) = rest.split_once('/') {
            let host = host.trim_end_matches(":8554");
            return format!("http://{host}:8888/{path}");
        }
    }
    if let Some(rest) = url.strip_prefix("rtmp://") {
        if let Some((host, path)) = rest.split_once('/') {
            let host = host.trim_end_matches(":1935");
            return format!("http://{host}:8888/{path}");
        }
    }
    url.to_string()
}

/// Remux an already-encoded MP4 to RTSP/RTMP (`ffmpeg -c copy`). No second video encode.
pub fn publish_file(path: &Path, url: &str) -> crate::Result<()> {
    let Some(args) = ffmpeg_args(path, url) else {
        return Ok(());
    };
    let mut cmd = Command::new("ffmpeg");
    cmd.args(&args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    match cmd.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(Error::Stream(format!("ffmpeg exited {status}"))),
        Err(err) => Err(Error::Stream(format!("ffmpeg: {err}"))),
    }
}

fn ffmpeg_args(path: &Path, url: &str) -> Option<Vec<String>> {
    let mut args = vec![
        "-hide_banner".into(),
        "-nostdin".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        path.display().to_string(),
        "-c".into(),
        "copy".into(),
    ];
    if url.starts_with("rtsp://") {
        args.extend([
            "-f".into(),
            "rtsp".into(),
            "-rtsp_transport".into(),
            "tcp".into(),
        ]);
    } else if url.starts_with("rtmp://") {
        args.extend(["-f".into(), "flv".into()]);
    } else {
        return None;
    }
    args.push(url.to_string());
    Some(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn view_url_maps_mediamtx_ports() {
        assert_eq!(
            view_url("rtsp://127.0.0.1:8554/live"),
            "http://127.0.0.1:8888/live"
        );
        assert_eq!(
            view_url("rtmp://127.0.0.1:1935/live/app"),
            "http://127.0.0.1:8888/live/app"
        );
        assert_eq!(
            view_url("http://127.0.0.1:8888/live"),
            "http://127.0.0.1:8888/live"
        );
    }

    #[test]
    fn ffmpeg_args_copy_only() {
        let args = ffmpeg_args(Path::new(r"C:\out.mp4"), "rtsp://127.0.0.1:8554/live").unwrap();
        assert!(args.windows(2).any(|w| w == ["-c", "copy"]));
        assert!(args.windows(2).any(|w| w == ["-f", "rtsp"]));
        assert_eq!(
            ffmpeg_args(Path::new("out.mp4"), "http://127.0.0.1:8888/live"),
            None
        );
    }
}
