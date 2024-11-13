use std::{fs::{read_link, remove_file}, io::{Read, Write}, net::TcpStream, os::unix::fs::symlink, process::Command};
use anyhow::anyhow;

pub fn is_healthy(port: u16) -> bool {
    fn is_healthy_inner(port: u16) -> anyhow::Result<()> {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}"))?;
        stream.write_all(b"GET / HTTP/1.1\r\n\r\n")?;
        let mut buf = [0; 1];
        let bytes = stream.read(&mut buf)?;
        if bytes == 0 {
            return Err(anyhow!("No response"));
        }

        Ok(())
    } 

    is_healthy_inner(port).is_ok()
}

pub fn checking_symlink(original: &str, link: &str) -> anyhow::Result<bool> {
    let previous_link = read_link(link)?;
    let expected_link = &original;

    if previous_link.to_str() == Some(expected_link) {
        return Ok(false);
    }

    // Replace nginx config with hibernator config
    remove_file(link).map_err(|e| anyhow!("could not remove previous symlink: {e}"))?;
    symlink(original, link).map_err(|e| anyhow!("could not create symlink: {e}"))?;
    Ok(true)
}

pub fn run_command(command: &str) -> anyhow::Result<()> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|e| anyhow!("could not run command: {e}"))?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("command failed: {command} {stdout} {stderr}"));
    }

    Ok(())
}
