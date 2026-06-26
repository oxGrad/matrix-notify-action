use anyhow::{anyhow, Result};
use std::fs::OpenOptions;
use std::io::Write;

pub fn write_event_id(event_id: &str) -> Result<()> {
    append_output(&format!("event_id={}\n", event_id))
}

pub fn write_error(msg: &str) -> Result<()> {
    append_output(&format!("error={}\n", msg))
}

fn append_output(line: &str) -> Result<()> {
    let path =
        std::env::var("GITHUB_OUTPUT").map_err(|_| anyhow!("GITHUB_OUTPUT env var is not set"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| anyhow!("Failed to open GITHUB_OUTPUT at {}: {}", path, e))?;
    file.write_all(line.as_bytes())
        .map_err(|e| anyhow!("Failed to write to GITHUB_OUTPUT: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn writes_event_id() {
        let f = NamedTempFile::new().unwrap();
        std::env::set_var("GITHUB_OUTPUT", f.path());
        write_event_id("$abc123:matrix.org").unwrap();
        let contents = fs::read_to_string(f.path()).unwrap();
        assert!(contents.contains("event_id=$abc123:matrix.org\n"));
    }

    #[test]
    fn writes_error() {
        let f = NamedTempFile::new().unwrap();
        std::env::set_var("GITHUB_OUTPUT", f.path());
        write_error("something went wrong").unwrap();
        let contents = fs::read_to_string(f.path()).unwrap();
        assert!(contents.contains("error=something went wrong\n"));
    }

    #[test]
    fn errors_when_github_output_not_set() {
        std::env::remove_var("GITHUB_OUTPUT");
        assert!(write_event_id("$x:y").is_err());
    }
}
