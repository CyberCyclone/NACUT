//! Windows: Claude Code installed inside WSL keeps its transcripts in the Linux home folder,
//! reachable as `\\wsl.localhost\<distro>\home\<user>\.claude\projects`.
//!
//! Only *running* distributions are checked, so NACUT never boots a WSL VM just to look.
//! Transcripts read earlier stay in the cache when a distribution stops.

use std::path::PathBuf;

/// `projects` folders of Claude Code inside running WSL distributions. Empty off Windows.
pub fn transcript_roots() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        running_distros().into_iter().flat_map(|d| roots_in(&d)).collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// `wsl.exe` prints UTF-16LE; this turns its output into one name per line.
pub fn decode_distro_list(bytes: &[u8]) -> Vec<String> {
    let text = if bytes.len() >= 2 && bytes.len() % 2 == 0 && bytes.iter().skip(1).step_by(2).all(|&b| b == 0) {
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    text.lines()
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0'))
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(windows)]
fn running_distros() -> Vec<String> {
    let mut cmd = std::process::Command::new("wsl.exe");
    cmd.args(["--list", "--running", "--quiet"]);
    super::no_window(&mut cmd);
    match cmd.output() {
        Ok(out) if out.status.success() => decode_distro_list(&out.stdout),
        _ => Vec::new(),
    }
}

#[cfg(windows)]
fn roots_in(distro: &str) -> Vec<PathBuf> {
    let base = PathBuf::from(format!(r"\\wsl.localhost\{distro}"));
    let mut homes: Vec<PathBuf> = std::fs::read_dir(base.join("home"))
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    homes.push(base.join("root"));
    homes
        .into_iter()
        .flat_map(|h| [h.join(".claude").join("projects"), h.join(".config").join("claude").join("projects")])
        .filter(|p| p.is_dir())
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_utf16_list() {
        let s = "Ubuntu\r\nDebian\r\n";
        let bytes: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(super::decode_distro_list(&bytes), ["Ubuntu", "Debian"]);
        assert_eq!(super::decode_distro_list(b"Ubuntu\n"), ["Ubuntu"]);
    }
}
